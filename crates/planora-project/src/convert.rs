//! Umwandlung zwischen Snapshot und den Format-Typen. `encode_version` schreibt nur, was sich
//! wieder lesen lässt; `decode_version` prüft alles, bevor etwas entsteht.

use crate::base64;
use crate::error::{ProjectError, Result};
use crate::format::*;
use planora_drawing::dimension::{
    ChainMode, ChainSpec, DimId, DimKind, DimSpec, EndSel, GeoRef, GeoRefData, TagData,
    Tolerance as DimTolerance, ref_count,
};
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, ProjectInfo, Revision, SheetInfo, validate_project_info,
    validate_sheet_info,
};
use planora_drawing::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewSpec,
};
use planora_exchange::{ExchangeMesh, MAX_FILE_BYTES};
use planora_feature::{
    BlendArt, BodyRef, EdgeCopy, EdgeRef, FaceRef, FeatureId, FeatureKind, MAX_COORD_MM,
    MAX_DECIMALS, MAX_DIM_TEXT_CHARS, MAX_DRAWINGS, MAX_FIT_CHARS, MAX_POS_MM, MAX_ROLE_CHARS,
    MAX_TOL_MM, MAX_VIEWS, MeshId, SketchFeature, SketchPlane, Snapshot, SnapshotFeature,
    StandardPlane,
};
use planora_kernel::BoolOp;
use planora_math::{DQuat, DVec3, Frame, Tolerance};
use planora_sketch::{
    ConstraintKind, ConstraintRecord, CurveId, CurveKind, CurveRecord, LineOrPoints, PointId,
    PointRecord, Sketch, SketchData,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Höchste lesbare Formatnummer der Versionsdateien (P7, P12, F7).
pub const FORMAT_VERSION: u32 = 6;
/// Formatnummer der Rahmendateien `dokument.json` und `.autosave.json`; ihre Struktur ist seit
/// Format 1 unverändert, ältere Programme öffnen das Projekt deshalb weiter (P7).
pub const CONTAINER_FORMAT: u32 = 1;
/// Größte Summe der dekodierten Netzdaten eines Dokuments (wie die Dateigrenze beim Import).
const MAX_MESH_BYTES: u64 = MAX_FILE_BYTES;

/// Kleinste Formatnummer, die den Stand darstellt: 6 mit Blatt- oder Projektangaben, 5 mit Maßen oder eingeschalteten
/// Mittellinien, 4 mit Zeichnungen, 3 mit Netz-Körpern, 2 mit Rundungen oder Fasen, sonst 1
/// (P7, P12, F7, Teilprojekt 12 P11).
pub fn required_format(s: &Snapshot) -> u32 {
    let has = |f: fn(&FeatureKind) -> bool| s.features.iter().any(|x| f(&x.kind));
    if s.project_info.is_some() || s.drawings.iter().any(|d| d.info.is_some()) {
        6
    } else if s.drawings.iter().any(needs_format_5) {
        5
    } else if !s.drawings.is_empty() {
        4
    } else if has(|k| matches!(k, FeatureKind::MeshBody { .. })) {
        3
    } else if has(|k| matches!(k, FeatureKind::Verrunden { .. })) {
        2
    } else {
        1
    }
}
/// Eine Zeichnung braucht Format 5, wenn sie Maße hat oder eine Ansicht Mittellinien zeigt.
fn needs_format_5(d: &Drawing) -> bool {
    !d.dimensions.is_empty() || d.views.iter().any(|v| v.centerlines)
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

// ---- Zeichnungen (Format 4 und 5) -------------------------------------------------------------

const PAPERS: [(Paper, &str); 5] = [
    (Paper::A4, "A4"),
    (Paper::A3, "A3"),
    (Paper::A2, "A2"),
    (Paper::A1, "A1"),
    (Paper::A0, "A0"),
];
const PROJECTIONS: [(Projection, &str); 2] =
    [(Projection::First, "erst"), (Projection::Third, "dritt")];
const VIEW_KINDS: [(ViewKind, &str); 7] = [
    (ViewKind::Front, "vorne"),
    (ViewKind::Back, "hinten"),
    (ViewKind::Left, "links"),
    (ViewKind::Right, "rechts"),
    (ViewKind::Top, "oben"),
    (ViewKind::Bottom, "unten"),
    (ViewKind::Isometric, "iso"),
];

fn name_of<T: PartialEq + Copy>(table: &[(T, &'static str)], x: T) -> String {
    table
        .iter()
        .find(|(v, _)| *v == x)
        .map(|(_, n)| (*n).to_string())
        .unwrap_or_default()
}

fn value_of<T: Copy>(table: &[(T, &'static str)], s: &str) -> Option<T> {
    table.iter().find(|(_, n)| *n == s).map(|(v, _)| *v)
}

/// Lageangabe auf dem Blatt: endlich und im Betrag höchstens `MAX_POS_MM`.
fn pos(origin: &str, what: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x.abs() <= MAX_POS_MM {
        Ok(x)
    } else {
        Err(ProjectError::corrupt(
            origin,
            format!("{what}: Zahl ungültig oder zu groß"),
        ))
    }
}

/// Zähler aus der Datei: 0 heißt „größte Kennung + 1“ (bzw. 1 ohne Kennungen). Jede Kennung
/// muss größer als 0 und kleiner als der Zähler sein; `u64::MAX` ist kein gültiger Zähler.
fn counter(
    origin: &str,
    what: &str,
    stored: u64,
    ids: impl Iterator<Item = u64> + Clone,
) -> Result<u64> {
    let bad_id = || ProjectError::corrupt(origin, format!("{what}: ungültige Kennung"));
    let next = if stored == 0 {
        match ids.clone().max() {
            Some(m) => m.checked_add(1).ok_or_else(bad_id)?,
            None => 1,
        }
    } else {
        stored
    };
    if next == u64::MAX {
        return Err(ProjectError::corrupt(
            origin,
            format!("{what}: Der Zähler ist ungültig"),
        ));
    }
    if ids.into_iter().any(|id| id == 0 || id >= next) {
        return Err(bad_id());
    }
    Ok(next)
}

// ---- Maße (Format 5) --------------------------------------------------------------------

const DIM_KINDS: [(DimKind, &str); 6] = [
    (DimKind::Horizontal, "waagerecht"),
    (DimKind::Vertical, "senkrecht"),
    (DimKind::Aligned, "schraeg"),
    (DimKind::Diameter, "durchmesser"),
    (DimKind::Radius, "radius"),
    (DimKind::Angle, "winkel"),
];
const END_SELS: [(EndSel, &str); 2] = [(EndSel::Start, "anfang"), (EndSel::End, "ende")];
const CHAIN_MODES: [(ChainMode, &str); 2] =
    [(ChainMode::Chain, "kette"), (ChainMode::Baseline, "bezug")];

fn tag_to(t: &TagData) -> FaceRefDto {
    FaceRefDto {
        herkunft: t.origin,
        rolle: t.role.clone(),
        hinweis: v3(t.hint),
    }
}

/// Bezug in der Datei; `None`, wenn die Daten nicht zur Art passen (Kantenbezug mit
/// Flächendaten oder umgekehrt), was die Prüfung des Dokuments ausschließt.
fn georef_to(r: &GeoRef) -> Option<BezugDto> {
    Some(match r {
        GeoRef::Edge(GeoRefData::Edge { a, b, hint }) => BezugDto::Kante {
            a: tag_to(a),
            b: tag_to(b),
            hinweis: v3(*hint),
        },
        GeoRef::EdgeEnd {
            edge: GeoRefData::Edge { a, b, hint },
            end,
        } => BezugDto::Kantenende {
            a: tag_to(a),
            b: tag_to(b),
            hinweis: v3(*hint),
            ende: name_of(&END_SELS, *end),
        },
        GeoRef::Center(GeoRefData::Edge { a, b, hint }) => BezugDto::Mitte {
            a: tag_to(a),
            b: tag_to(b),
            hinweis: v3(*hint),
        },
        GeoRef::Face(GeoRefData::Face(t)) => BezugDto::Flaeche { marke: tag_to(t) },
        _ => return None,
    })
}

fn tolerance_to(t: &DimTolerance) -> ToleranzDto {
    match t {
        DimTolerance::None => ToleranzDto::Keine,
        DimTolerance::Symmetric(wert) => ToleranzDto::Symmetrisch { wert: *wert },
        DimTolerance::Limits { upper, lower } => ToleranzDto::Abmasse {
            oben: *upper,
            unten: *lower,
        },
        DimTolerance::LimitSizes { upper, lower } => ToleranzDto::Grenzmasse {
            oben: *upper,
            unten: *lower,
        },
        DimTolerance::Fit(text) => ToleranzDto::Passung { text: text.clone() },
    }
}

fn dimension_to(m: &DimSpec) -> Result<MassDto> {
    let bezuege = m
        .refs
        .iter()
        .map(|r| {
            georef_to(r).ok_or_else(|| {
                ProjectError::Invalid(format!(
                    "Ein Bezug des Maßes {} passt nicht zu seiner Art",
                    m.id.0
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(MassDto {
        id: m.id.0,
        ansicht: m.view.0,
        art: name_of(&DIM_KINDS, m.kind),
        bezuege,
        abstand: m.offset,
        winkel: m.angle,
        textversatz: [m.text_shift.0, m.text_shift.1],
        toleranz: tolerance_to(&m.tolerance),
        stellen: m.decimals,
        text: m.text_override.clone(),
        kette: m.chain.map(|c| KetteDto {
            gruppe: c.group,
            modus: name_of(&CHAIN_MODES, c.mode),
            reihenfolge: c.order,
        }),
        vorgeschlagen: m.suggested,
    })
}

/// Hinweispunkt eines Maßbezugs: dieselbe Regel wie bei den übrigen Verweisen ([`num`]:
/// endlich, Betrag höchstens `2·MAX_COORD_MM` = `MAX_HINT_MM`), denn es ist ein Modellpunkt,
/// kein Blattwert.
fn hint_from(origin: &str, what: &str, v: V3) -> Result<DVec3> {
    dv3(origin, &format!("{what} Hinweispunkt"), v)
}

/// Liest eine Flächenmarke eines Maßbezugs: Rolle höchstens [`MAX_ROLE_CHARS`] Zeichen,
/// Hinweispunkt wie in [`hint_from`].
fn tag_from(origin: &str, what: &str, d: &FaceRefDto) -> Result<TagData> {
    if d.rolle.chars().count() > MAX_ROLE_CHARS {
        return Err(ProjectError::corrupt(
            origin,
            format!(
                "{what}: Die Rolle einer Fläche ist zu lang (höchstens {MAX_ROLE_CHARS} Zeichen)"
            ),
        ));
    }
    Ok(TagData {
        origin: d.herkunft,
        role: d.rolle.clone(),
        hint: hint_from(origin, what, d.hinweis)?,
    })
}

fn edge_data_from(
    origin: &str,
    what: &str,
    a: &FaceRefDto,
    b: &FaceRefDto,
    hinweis: V3,
) -> Result<GeoRefData> {
    Ok(GeoRefData::Edge {
        a: tag_from(origin, what, a)?,
        b: tag_from(origin, what, b)?,
        hint: hint_from(origin, what, hinweis)?,
    })
}

fn georef_from(origin: &str, what: &str, d: &BezugDto) -> Result<GeoRef> {
    Ok(match d {
        BezugDto::Kante { a, b, hinweis } => {
            GeoRef::Edge(edge_data_from(origin, what, a, b, *hinweis)?)
        }
        BezugDto::Kantenende {
            a,
            b,
            hinweis,
            ende,
        } => GeoRef::EdgeEnd {
            end: value_of(&END_SELS, ende).ok_or_else(|| {
                ProjectError::corrupt(origin, format!("{what}: Unbekanntes Kantenende „{ende}“"))
            })?,
            edge: edge_data_from(origin, what, a, b, *hinweis)?,
        },
        BezugDto::Mitte { a, b, hinweis } => {
            GeoRef::Center(edge_data_from(origin, what, a, b, *hinweis)?)
        }
        BezugDto::Flaeche { marke } => {
            GeoRef::Face(GeoRefData::Face(tag_from(origin, what, marke)?))
        }
    })
}

fn tolerance_from(origin: &str, what: &str, d: &ToleranzDto) -> Result<DimTolerance> {
    let t = |x: f64| {
        if x.is_finite() && x.abs() <= MAX_TOL_MM {
            Ok(x)
        } else {
            Err(ProjectError::corrupt(
                origin,
                format!("{what} Toleranz: Zahl ungültig oder zu groß"),
            ))
        }
    };
    Ok(match d {
        ToleranzDto::Keine => DimTolerance::None,
        ToleranzDto::Symmetrisch { wert } => DimTolerance::Symmetric(t(*wert)?),
        ToleranzDto::Abmasse { oben, unten } => DimTolerance::Limits {
            upper: t(*oben)?,
            lower: t(*unten)?,
        },
        ToleranzDto::Grenzmasse { oben, unten } => DimTolerance::LimitSizes {
            upper: t(*oben)?,
            lower: t(*unten)?,
        },
        ToleranzDto::Passung { text } => {
            if text.chars().count() > MAX_FIT_CHARS {
                return Err(ProjectError::corrupt(
                    origin,
                    format!(
                        "{what}: Die Passungsangabe ist zu lang (höchstens {MAX_FIT_CHARS} Zeichen)"
                    ),
                ));
            }
            DimTolerance::Fit(text.clone())
        }
    })
}

/// Liest ein Maß. `views`: Ansichten der Zeichnung mit ihrer Art (das Maß muss zu einer
/// Normansicht gehören).
fn dimension_from(
    origin: &str,
    drawing: &str,
    views: &HashMap<ViewId, ViewKind>,
    d: &MassDto,
) -> Result<DimSpec> {
    let what = format!("Zeichnung „{drawing}“, Maß {}", d.id);
    let bad = |m: String| ProjectError::corrupt(origin, format!("{what}: {m}"));
    let kind = value_of(&DIM_KINDS, &d.art)
        .ok_or_else(|| bad(format!("Unbekannte Maßart „{}“", d.art)))?;
    match views.get(&ViewId(d.ansicht)) {
        None => return Err(bad("Das Maß gehört zu keiner Ansicht der Zeichnung".into())),
        Some(k) if !k.is_normal() => {
            return Err(bad("In der isometrischen Ansicht gibt es keine Maße".into()));
        }
        Some(_) => {}
    }
    let (lo, hi) = ref_count(kind);
    if d.bezuege.len() < lo || d.bezuege.len() > hi {
        return Err(bad("Das Maß hat die falsche Zahl von Bezügen".into()));
    }
    let refs = d
        .bezuege
        .iter()
        .map(|r| georef_from(origin, &what, r))
        .collect::<Result<Vec<_>>>()?;
    if d.stellen > MAX_DECIMALS {
        return Err(bad(format!(
            "Ein Maß darf höchstens {MAX_DECIMALS} Nachkommastellen haben"
        )));
    }
    if d.text
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_DIM_TEXT_CHARS)
    {
        return Err(bad(format!(
            "Der eigene Maßtext ist zu lang (höchstens {MAX_DIM_TEXT_CHARS} Zeichen)"
        )));
    }
    // Eine Stelle in einer Kette ist höchstens so groß wie die Zahl der Maße einer Zeichnung
    // (gestapelte Bezugsmaße liegen sonst unbegrenzt weit draußen).
    if let Some(k) = &d.kette
        && k.reihenfolge as usize >= MAX_DIMENSIONS
    {
        return Err(bad(format!(
            "Die Stelle in der Maßkette ist zu groß (höchstens {})",
            MAX_DIMENSIONS - 1
        )));
    }
    let chain = match &d.kette {
        None => None,
        Some(k) => Some(ChainSpec {
            group: k.gruppe,
            mode: value_of(&CHAIN_MODES, &k.modus)
                .ok_or_else(|| bad(format!("Unbekannte Kettenart „{}“", k.modus)))?,
            order: k.reihenfolge,
        }),
    };
    let p = |name: &str, x: f64| pos(origin, &format!("{what} {name}"), x);
    Ok(DimSpec {
        id: DimId(d.id),
        view: ViewId(d.ansicht),
        kind,
        refs,
        offset: p("Abstand", d.abstand)?,
        angle: p("Winkel", d.winkel)?,
        text_shift: (
            p("Textversatz", d.textversatz[0])?,
            p("Textversatz", d.textversatz[1])?,
        ),
        tolerance: tolerance_from(origin, &what, &d.toleranz)?,
        decimals: d.stellen,
        text_override: d.text.clone(),
        chain,
        suggested: d.vorgeschlagen,
    })
}

/// Liest die Maße einer Zeichnung samt Maßzähler: Anzahl begrenzt, Kennungen größer als 0,
/// kleiner als der Zähler und eindeutig, Maße einer Gruppe mit derselben Kettenart. Die
/// übrigen Regeln (gleiche Art und Ansicht in einer Gruppe, Stellen eindeutig) prüft danach
/// `validate_drawing` beim Laden des Dokuments.
fn dimensions_from(
    origin: &str,
    d: &ZeichnungDto,
    views: &HashMap<ViewId, ViewKind>,
) -> Result<(Vec<DimSpec>, u64)> {
    let ctx = |m: &str| ProjectError::corrupt(origin, format!("Zeichnung „{}“: {m}", d.name));
    if d.masse.len() > MAX_DIMENSIONS {
        return Err(ctx("Maße: zu viele Einträge"));
    }
    let next = counter(
        origin,
        &format!("Zeichnung „{}“, Maß", d.name),
        d.naechstes_mass,
        d.masse.iter().map(|m| m.id),
    )?;
    let mut ids = HashSet::with_capacity(d.masse.len());
    let mut modes: HashMap<u64, ChainMode> = HashMap::new();
    let mut out = Vec::with_capacity(d.masse.len());
    for m in &d.masse {
        if !ids.insert(m.id) {
            return Err(ctx("Eine Maßkennung kommt doppelt vor"));
        }
        let spec = dimension_from(origin, &d.name, views, m)?;
        if let Some(c) = spec.chain
            && *modes.entry(c.group).or_insert(c.mode) != c.mode
        {
            return Err(ctx(
                "Die Maße einer Kette mischen Kettenmaße und Bezugsmaße",
            ));
        }
        out.push(spec);
    }
    Ok((out, next))
}

fn field_to(v: &FieldValue) -> Option<String> {
    match v {
        FieldValue::Auto => None,
        FieldValue::Text(t) => Some(t.clone()),
    }
}

fn field_from(v: &Option<String>) -> FieldValue {
    v.as_ref()
        .map_or(FieldValue::Auto, |t| FieldValue::Text(t.clone()))
}

fn notes_to(n: &NotesValue) -> Option<Vec<String>> {
    match n {
        NotesValue::Auto => None,
        NotesValue::Lines(l) => Some(l.clone()),
    }
}

fn notes_from(n: &Option<Vec<String>>) -> NotesValue {
    n.as_ref()
        .map_or(NotesValue::Auto, |l| NotesValue::Lines(l.clone()))
}

fn project_to(p: &ProjectInfo) -> ProjektangabenDto {
    ProjektangabenDto {
        anmeldename: p.login.clone(),
        eigentuemer: field_to(&p.owner),
        ersteller: field_to(&p.creator),
        geprueft: field_to(&p.checked),
        freigabe: field_to(&p.approved),
        werkstoff: field_to(&p.material),
        allgemeintoleranz: field_to(&p.tolerance),
        hinweise: notes_to(&p.notes),
    }
}

fn project_from(origin: &str, p: &ProjektangabenDto) -> Result<ProjectInfo> {
    let info = ProjectInfo {
        login: p.anmeldename.clone(),
        owner: field_from(&p.eigentuemer),
        creator: field_from(&p.ersteller),
        checked: field_from(&p.geprueft),
        approved: field_from(&p.freigabe),
        material: field_from(&p.werkstoff),
        tolerance: field_from(&p.allgemeintoleranz),
        notes: notes_from(&p.hinweise),
    };
    validate_project_info(&info)
        .map_err(|e| ProjectError::corrupt(origin, format!("Projektangaben: {e}")))?;
    Ok(info)
}

fn sheet_info_to(i: &SheetInfo) -> BlattangabenDto {
    BlattangabenDto {
        angelegt: i.created.clone(),
        nummer: field_to(&i.number),
        titel: field_to(&i.title),
        zusatztitel: field_to(&i.subtitle),
        dokumentart: field_to(&i.doc_type),
        dokumentstatus: field_to(&i.status),
        ausgabedatum: field_to(&i.issued),
        aenderungsstand: field_to(&i.revision),
        blatt: field_to(&i.sheet_of),
        format: field_to(&i.format),
        massstab: field_to(&i.scale),
        gewicht: field_to(&i.weight),
        hinweise: notes_to(&i.notes),
        aenderungen: i
            .revisions
            .iter()
            .map(|r| AenderungDto {
                buchstabe: field_to(&r.letter),
                beschreibung: r.description.clone(),
                datum: r.date.clone(),
                name: r.name.clone(),
            })
            .collect(),
    }
}

fn sheet_info_from(origin: &str, name: &str, b: &BlattangabenDto) -> Result<SheetInfo> {
    let info = SheetInfo {
        created: b.angelegt.clone(),
        number: field_from(&b.nummer),
        title: field_from(&b.titel),
        subtitle: field_from(&b.zusatztitel),
        doc_type: field_from(&b.dokumentart),
        status: field_from(&b.dokumentstatus),
        issued: field_from(&b.ausgabedatum),
        revision: field_from(&b.aenderungsstand),
        sheet_of: field_from(&b.blatt),
        format: field_from(&b.format),
        scale: field_from(&b.massstab),
        weight: field_from(&b.gewicht),
        notes: notes_from(&b.hinweise),
        revisions: b
            .aenderungen
            .iter()
            .map(|a| Revision {
                letter: field_from(&a.buchstabe),
                description: a.beschreibung.clone(),
                date: a.datum.clone(),
                name: a.name.clone(),
            })
            .collect(),
    };
    validate_sheet_info(&info)
        .map_err(|e| ProjectError::corrupt(origin, format!("Zeichnung „{name}“: {e}")))?;
    Ok(info)
}

fn drawing_to(d: &Drawing, with_marks: bool) -> Result<ZeichnungDto> {
    Ok(ZeichnungDto {
        id: d.id.0,
        name: d.name.clone(),
        blatt: BlattDto {
            papier: name_of(&PAPERS, d.sheet.paper),
            quer: d.sheet.landscape,
        },
        projektion: name_of(&PROJECTIONS, d.projection),
        naechste_ansicht: d.next_view,
        ansichten: d
            .views
            .iter()
            .map(|v| AnsichtDto {
                id: v.id.0,
                art: name_of(&VIEW_KINDS, v.kind),
                massstab: [v.scale.num, v.scale.den],
                lage: match v.placement {
                    Placement::Free { x, y } => LageDto::Frei { x, y },
                    Placement::Aligned { distance } => LageDto::Ausgerichtet { abstand: distance },
                },
                verdeckt_zeigen: v.show_hidden,
                eltern: v.parent.map(|p| p.0),
                mittellinien: with_marks.then_some(v.centerlines),
            })
            .collect(),
        masse: if with_marks {
            d.dimensions
                .iter()
                .map(dimension_to)
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        },
        // Bis Format 4 fehlt der Zähler, damit diese Dateien unverändert bleiben; ohne Maße und
        // Mittellinien geht er beim Speichern verloren (Kennungen beginnen dann wieder bei 1).
        naechstes_mass: if with_marks { d.next_dim } else { 0 },
        blattangaben: d.info.as_ref().map(sheet_info_to),
    })
}

fn drawing_from(origin: &str, d: &ZeichnungDto) -> Result<Drawing> {
    let ctx = |m: String| ProjectError::corrupt(origin, format!("Zeichnung „{}“: {m}", d.name));
    if d.ansichten.len() > MAX_VIEWS {
        return Err(ctx("Ansichten: zu viele Einträge".into()));
    }
    let paper = value_of(&PAPERS, &d.blatt.papier)
        .ok_or_else(|| ctx(format!("Unbekanntes Papierformat „{}“", d.blatt.papier)))?;
    let projection = value_of(&PROJECTIONS, &d.projektion)
        .ok_or_else(|| ctx(format!("Unbekannte Projektion „{}“", d.projektion)))?;
    let what = format!("Zeichnung „{}“, Ansicht", d.name);
    let next_view = counter(
        origin,
        &what,
        d.naechste_ansicht,
        d.ansichten.iter().map(|v| v.id),
    )?;
    let mut views = Vec::with_capacity(d.ansichten.len());
    for v in &d.ansichten {
        let kind = value_of(&VIEW_KINDS, &v.art)
            .ok_or_else(|| ctx(format!("Unbekannte Ansichtsart „{}“", v.art)))?;
        let [num, den] = v.massstab;
        if !(1..=1000).contains(&num) || !(1..=1000).contains(&den) {
            return Err(ctx(format!(
                "Maßstab {num}:{den} liegt nicht zwischen 1:1000 und 1000:1"
            )));
        }
        let p = |what: &str, x: f64| pos(origin, &format!("Zeichnung „{}“ {what}", d.name), x);
        let placement = match v.lage {
            LageDto::Frei { x, y } => Placement::Free {
                x: p("Lage", x)?,
                y: p("Lage", y)?,
            },
            LageDto::Ausgerichtet { abstand } => Placement::Aligned {
                distance: p("Abstand", abstand)?,
            },
        };
        views.push(ViewSpec {
            id: ViewId(v.id),
            kind,
            scale: Scale { num, den },
            placement,
            show_hidden: v.verdeckt_zeigen,
            parent: v.eltern.map(ViewId),
            centerlines: v.mittellinien.unwrap_or(false),
        });
    }
    let kinds: HashMap<ViewId, ViewKind> = views.iter().map(|v| (v.id, v.kind)).collect();
    let (dimensions, next_dim) = dimensions_from(origin, d, &kinds)?;
    Ok(Drawing {
        id: DrawingId(d.id),
        name: d.name.clone(),
        sheet: Sheet {
            paper,
            landscape: d.blatt.quer,
        },
        projection,
        views,
        next_view,
        dimensions,
        next_dim,
        info: d
            .blattangaben
            .as_ref()
            .map(|b| sheet_info_from(origin, &d.name, b))
            .transpose()?,
    })
}

/// Liest die Zeichnungen und den Zeichnungszähler. Weitere Prüfungen (doppelte Kennungen und
/// Namen, Ausgangsansichten) macht `Document::from_snapshot`.
fn drawings_from(origin: &str, file: &VersionFile) -> Result<(Vec<Drawing>, u64)> {
    if file.format < 4 && (!file.zeichnungen.is_empty() || file.naechste_zeichnung != 0) {
        return Err(ProjectError::corrupt(
            origin,
            "Zeichnungen gibt es erst ab Dateiformat 4",
        ));
    }
    let has_marks = |z: &ZeichnungDto| {
        !z.masse.is_empty()
            || z.naechstes_mass != 0
            || z.ansichten.iter().any(|v| v.mittellinien.is_some())
    };
    if file.format < 5 && file.zeichnungen.iter().any(has_marks) {
        return Err(ProjectError::corrupt(
            origin,
            "Maße und Mittellinien gibt es erst ab Dateiformat 5",
        ));
    }
    if file.format < 6 && file.zeichnungen.iter().any(|z| z.blattangaben.is_some()) {
        return Err(ProjectError::corrupt(
            origin,
            "Blattangaben gibt es erst ab Dateiformat 6",
        ));
    }
    check_len(origin, "Zeichnungen", file.zeichnungen.len())?;
    if file.zeichnungen.len() > MAX_DRAWINGS {
        return Err(ProjectError::corrupt(
            origin,
            "Zeichnungen: zu viele Einträge",
        ));
    }
    let next = counter(
        origin,
        "Zeichnung",
        file.naechste_zeichnung,
        file.zeichnungen.iter().map(|z| z.id),
    )?;
    let drawings = file
        .zeichnungen
        .iter()
        .map(|z| drawing_from(origin, z))
        .collect::<Result<Vec<_>>>()?;
    Ok((drawings, next))
}

// ---- Öffentliche Funktionen -------------------------------------------------------------

/// JSON-Text einer Versionsdatei. Enthält der Snapshot eine nicht endliche Zahl, ist das ein
/// Fehler (das Dokument wird dann nicht geschrieben).
pub fn encode_version(s: &Snapshot) -> Result<String> {
    let format = required_format(s);
    let file = VersionFile {
        format,
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
        zeichnungen: s
            .drawings
            .iter()
            .map(|d| drawing_to(d, format >= 5))
            .collect::<Result<Vec<_>>>()?,
        // Ohne Zeichnungen fehlt der Zähler, damit alte Dateien unverändert bleiben (F7).
        naechste_zeichnung: if s.drawings.is_empty() {
            0
        } else {
            s.next_drawing
        },
        projektangaben: s.project_info.as_ref().map(project_to),
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
    if file.format < 6 && file.projektangaben.is_some() {
        return Err(ProjectError::corrupt(
            origin,
            "Blattangaben gibt es erst ab Dateiformat 6",
        ));
    }
    let (drawings, next_drawing) = drawings_from(origin, &file)?;
    let project_info = file
        .projektangaben
        .as_ref()
        .map(|p| project_from(origin, p))
        .transpose()?;
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
        drawings,
        next_drawing,
        project_info,
    })
}
