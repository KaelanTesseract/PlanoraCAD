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
use planora_drawing::parts::{MAX_BALLOONS, validate_override};
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, ProjectInfo, Revision, SheetInfo, validate_project_info,
    validate_sheet_info,
};
use planora_drawing::{
    Balloon, BalloonId, BodyKey, Drawing, DrawingId, DrawingKind, HatchSpec, Paper, Placement,
    Projection, RowOverride, Scale, SectionSpec, Sheet, ViewId, ViewKind, ViewSpec,
};
use planora_exchange::{
    ExchangeError, ExchangeMesh, ImageFormat, MAX_FILE_BYTES, MAX_IMAGE_BYTES,
    MAX_IMAGES_TOTAL_BYTES, probe_image,
};
use planora_feature::planes::{AxisRef, PlaneBase, PlaneDef, PlaneFeature, PointRef, StdAxis};
use planora_feature::{
    BlendArt, BodyRef, CanvasBase, CanvasFeature, CanvasImage, CopySource, CopyTarget, CurveSource,
    EdgeCopy, EdgeRef, FaceRef, FaceTarget, FeatureId, FeatureKind, FieldKey, Formulas, ImageId,
    IncludeRef, IntersectTarget, IntersectionSpec, MAX_COMMENT_CHARS, MAX_COORD_MM, MAX_DECIMALS,
    MAX_DIM_TEXT_CHARS, MAX_DRAWINGS, MAX_FIT_CHARS, MAX_FORMULA_CHARS, MAX_PARAM_NAME_CHARS,
    MAX_PARAMETERS, MAX_POS_MM, MAX_ROLE_CHARS, MAX_TOL_MM, MAX_VIEWS, MeshId, ParamId, ParamUnit,
    Parameter, PlaneId, ProfileSel, ProjectType, SketchFeature, SketchFormulas, SketchPlane,
    Snapshot, SnapshotFeature, StandardPlane, StepShape, StepShapeId, SurfaceProjection, VertexRef,
    Visibility, field_keys, image_hash, step_text_megabytes, step_text_sum_limit, validate_drawing,
};
use planora_feature::{
    IsoDirection, SketchGroup, SketchGroupId, SketchGroups, SpunAxis, SpunSelection, SpunSpec,
    TextFormulas, has_param_ref,
};
use planora_i18n::{DisplayUnit, tr};
use planora_kernel::BoolOp;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DQuat, DVec2, DVec3, Frame, Tolerance};
use planora_sketch::comb::CombSettings;
use planora_sketch::pattern::{CircularMode, PatternDir, PatternInstance, PatternKind};
use planora_sketch::text::{HAlign, TextId, TextPlacement, VAlign};
use planora_sketch::text_shape::TextStyle;
use planora_sketch::{
    ConstraintId, ConstraintKind, ConstraintRecord, CurveId, CurveKind, CurveRecord,
    HandleCurvature, LegacyMigration, LineOrPoints, PatternId, PatternRecord, PatternSlot,
    PieceRef, PointId, PointRecord, ProfileRef, Sketch, SketchData, SplineHandle, TextRecord,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Höchste lesbare Formatnummer der Versionsdateien (P7, P12, F7).
pub const FORMAT_VERSION: u32 = 22;
/// Formatnummer der Rahmendateien `dokument.json` und `.autosave.json`; ihre Struktur ist seit
/// Format 1 unverändert, ältere Programme öffnen das Projekt deshalb weiter (P7).
pub const CONTAINER_FORMAT: u32 = 1;
/// Größte Summe der dekodierten Netzdaten eines Dokuments (wie die Dateigrenze beim Import).
const MAX_MESH_BYTES: u64 = MAX_FILE_BYTES;
/// Größte Summe der Bildbytes eines Dokuments (Leinwände, Format 19): 1 GiB
/// ([`MAX_IMAGES_TOTAL_BYTES`], Festlegung Task 9; dieselbe Grenze prüft die App schon beim
/// Einfügen). Ein einzelnes Bild ist zusätzlich auf `MAX_IMAGE_BYTES` begrenzt.
const MAX_IMAGES_BYTES: u64 = MAX_IMAGES_TOTAL_BYTES;

/// Kleinste Formatnummer, die den Stand darstellt (die Regeln in dieser Reihenfolge geprüft): 22
/// mit einer Skizzengruppe, einer Höhenformel an einem Text, einem `{Name}`-Verweis in einem Text
/// (auch einem gelöschten), einem Drehprofil oder einer Isokurve ([`needs_format_22`]), 21
/// mit Krümmungskamm, Gegenwinkel, Tangente am Ende oder Glatt am Ellipsenbogen in einer Skizze
/// ([`needs_format_21`], auch in gelöschten Einträgen) oder einer Kopie mit fehlender oder
/// dahinterstehender Quelle ([`has_lost_source`]), 20
/// mit 3D-Geometrie oder dem Haken „3D-Skizze“ in einer Skizze ([`needs_format_20`]), 19
/// mit einer Leinwand (auch unterdrückt: ihr Bild muss in der Datei stehen) oder einem
/// Kontrollpunktrahmen an einer Skizzenkurve ([`needs_format_19`], auch an einem gelöschten
/// Kurveneintrag), 18
/// mit einem Text in einer Skizze ([`needs_format_18`], auch einem gelöschten Texteintrag), 17
/// mit einer der neuen Kurvenarten aus Skizzen 7 ([`needs_format_17`], auch einem gelöschten
/// Kurveneintrag), 16 mit einer Konstruktionsebene, einer Skizze auf einer Konstruktionsebene,
/// einer Schnitt- oder Silhouettenprojektion ([`needs_format_16`]) oder einer vom Grundzustand abweichenden
/// Sichtbarkeit, 15 mit einem Muster oder einer der
/// Abhängigkeitsarten aus Skizzen 5 in einer Skizze (auch einem aufgelösten Muster oder
/// gelöschten Eintrag), 14 mit einer Mittellinie in einer Skizze (auch
/// einem gelöschten Kurveneintrag), 13 mit einem Ellipsenbogen in einer Skizze (auch einem
/// gelöschten Kurveneintrag) oder mit Profil-Verweisen,
/// die sich nicht als alte Profilnummer schreiben lassen ([`ProfileField`]), 12 mit einer Zeichnung in
/// Zoll, 11 mit Rückrollstellen, 10 mit einem STEP-Körper (auch unterdrückt: sein Text muss in der Datei stehen),
/// 9 mit Parametern, Feature-Formeln oder Skizzenformeln, 8 mit Schnittansichten, 7 mit
/// Stücklisten-Daten (eigene Zeilentexte, Stücklistenblatt, Nummernkreise oder eingeschalteter
/// Stückliste auf einem Blatt), 6 mit Blatt- oder Projektangaben, 5 mit Maßen oder
/// eingeschalteten Mittellinien, 4 mit Zeichnungen, 3 mit Netz-Körpern, 2 mit Rundungen oder
/// Fasen, sonst 1 (P7, P12, F7, Teilprojekt 12 P11, Teilprojekt STEP-Import, Verlaufsleiste,
/// Teilprojekt Zoll, Teilprojekte Skizzen 1, 3, 5, 6, 7, 8, 9, 10, 11a und 11b).
pub fn required_format(s: &Snapshot) -> u32 {
    format_with(s, &profile_fields(s))
}

/// [`required_format`] mit den schon bestimmten Profilfeldern `fields` (eins je Feature).
fn format_with(s: &Snapshot, fields: &[ProfileField]) -> u32 {
    let has = |f: fn(&FeatureKind) -> bool| s.features.iter().any(|x| f(&x.kind));
    // Der Ordner „Leinwände“ kann nur mit einer Leinwand ausgeblendet sein (das Dokument räumt
    // ihn mit der letzten Leinwand weg); er braucht darum keine eigene Regel.
    if has(needs_format_22) || !s.sketch_groups.is_empty() {
        22
    } else if has(needs_format_21) || has_lost_source(&s.features) {
        21
    } else if has(needs_format_20) {
        20
    } else if has(needs_format_19) {
        19
    } else if has(needs_format_18) {
        18
    } else if has(needs_format_17) {
        17
    } else if has(needs_format_16) || !s.visibility.is_default() {
        16
    } else if has(|k| matches!(k, FeatureKind::Sketch(sf) if sf.sketch.needs_format_15())) {
        15
    } else if has(|k| matches!(k, FeatureKind::Sketch(sf) if sf.sketch.has_centerline_entry())) {
        14
    } else if has(|k| matches!(k, FeatureKind::Sketch(sf) if has_elliptic_arc(&sf.sketch)))
        || fields.iter().any(|f| f.profile.is_some())
    {
        13
    } else if s.drawings.iter().any(|d| d.unit != DisplayUnit::Millimeter) {
        12
    } else if !s.rollbacks.is_empty() {
        11
    } else if has(|k| matches!(k, FeatureKind::StepBody { .. })) {
        10
    } else if !s.parameters.is_empty()
        || s.features.iter().any(|f| !f.formulas.is_empty())
        || has(|k| matches!(k, FeatureKind::Sketch(sf) if !sf.formulas.is_empty()))
    {
        9
    } else if s
        .drawings
        .iter()
        .any(|d| d.views.iter().any(|v| v.section.is_some()))
    {
        8
    } else if !s.parts_overrides.is_empty() || s.drawings.iter().any(has_parts_data) {
        7
    } else if s.project_info.is_some() || s.drawings.iter().any(|d| d.info.is_some()) {
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
/// Braucht die Art Format 22 (Skizzen 11b)? Eine Skizze mit Höhenformel an einem Text, einem
/// `{Name}`-Verweis in einem Text (auch einem gelöschten: Er steht in der Datei) oder einer Kopie
/// mit Drehprofil oder Isokurve. Die Skizzengruppen prüft [`format_with`] am Snapshot.
fn needs_format_22(k: &FeatureKind) -> bool {
    matches!(k, FeatureKind::Sketch(sf) if !sf.text_formulas.is_empty()
    || sf.sketch.text_entry_contents().any(has_param_ref)
    || sf.copies.iter().any(|c| matches!(
        c.source,
        CopySource::SpunProfile(_) | CopySource::Isoparametric { .. }
    )))
}
/// Braucht die Art Format 21 (Skizzen 11a)? Eine Skizze mit Krümmungskamm, Tangente am Ende,
/// Gegenwinkel oder Glatt an einem Ellipsenbogen, auch in einem gelöschten Eintrag: Er steht in
/// der Datei ([`Sketch::needs_format_21`]).
fn needs_format_21(k: &FeatureKind) -> bool {
    matches!(k, FeatureKind::Sketch(sf) if sf.sketch.needs_format_21())
}
/// Hat eine Skizze eine Kopie, deren Herkunft (die Features aus [`CopySource::origins`], ohne
/// die losen Quellskizzen) im Verlauf fehlt oder nicht vor der Skizze steht (Skizzen 11a,
/// verlorene Projektion; Schlussprüfung I-2)? Ältere Programme prüfen diese Bezüge beim Laden
/// hart und brächen ab; die Datei braucht darum Format 21.
fn has_lost_source(features: &[SnapshotFeature]) -> bool {
    features.iter().enumerate().any(|(i, f)| {
        let FeatureKind::Sketch(sf) = &f.kind else {
            return false;
        };
        sf.copies
            .iter()
            .flat_map(|c| c.source.origins())
            .any(|o| !features[..i].iter().any(|g| g.id == o))
    })
}
/// Braucht die Art Format 20 (Skizzen 10)? Eine Skizze mit 3D-Geometrie (3D-Punkt, Kurve mit
/// Normale, Freiformkurve im Raum, `AlongZ`, `Fixed3`, die internen `InPlane`/`EllipseAxes3`)
/// oder dem Haken „3D-Skizze“, auch in einem gelöschten Eintrag: Er steht in der Datei
/// ([`Sketch::has_3d_entry`]); dazu eine Kopie mit einer der neuen Herkunftsarten, einem
/// Stückindex oder einem Punkt als Ziel (Ruling R3).
fn needs_format_20(k: &FeatureKind) -> bool {
    matches!(k, FeatureKind::Sketch(sf) if sf.sketch.has_3d_entry()
    || sf.copies.iter().any(|c| {
        c.source.is_3d_kind() || c.piece != 0 || c.of > 1 || c.point().is_some()
    }))
}
/// Braucht die Art Format 19 (Skizzen 9)? Eine Leinwand oder eine Skizze mit einem
/// Kontrollpunktrahmen, auch an einem gelöschten Kurveneintrag: Er steht in der Datei.
fn needs_format_19(k: &FeatureKind) -> bool {
    match k {
        FeatureKind::Canvas(_) => true,
        FeatureKind::Sketch(sf) => sf.sketch.has_frame_entry(),
        _ => false,
    }
}
/// Braucht die Art Format 18 (Skizzen 8)? Eine Skizze mit einem Text, auch einem gelöschten
/// Eintrag: Er steht in der Datei.
fn needs_format_18(k: &FeatureKind) -> bool {
    matches!(k, FeatureKind::Sketch(sf) if sf.sketch.has_text_entry())
}
/// Braucht die Art Format 17 (Skizzen 7)? Eine Skizze mit einem Spline mit Kontrollpunkten, einem
/// Kegelschnitt, einem festen Spline oder einem Griff, auch einem gelöschten Eintrag: Er steht in der Datei.
fn needs_format_17(k: &FeatureKind) -> bool {
    matches!(k, FeatureKind::Sketch(sf) if sf
    .sketch
    .curve_entries()
    .any(|c| match c {
        CurveKind::ControlSpline { .. } | CurveKind::Conic { .. } | CurveKind::FixedSpline { .. } => {
            true
        }
        CurveKind::Spline { handles, .. } => !handles.is_empty(),
        _ => false,
    }) || sf
        .sketch
        .constraint_entries()
        .any(|c| matches!(c, ConstraintKind::SplineCurvatureHandle(..))))
}
/// Braucht die Art Format 16 (Skizzen 6)? Konstruktionsebenen, Skizzen auf ihnen und Schnitt-
/// oder Silhouettenprojektionen. Kantenprojektionen gibt es schon länger, auch verknüpfte, nicht
/// zur Konstruktion gemachte Kurven (das steht an der Kurve, nicht an der Kopie).
fn needs_format_16(k: &FeatureKind) -> bool {
    match k {
        FeatureKind::Plane(_) => true,
        FeatureKind::Sketch(sf) => {
            matches!(sf.plane, SketchPlane::OnPlane(_))
                || sf
                    .copies
                    .iter()
                    .any(|c| !matches!(c.source, CopySource::Edge(_)))
        }
        _ => false,
    }
}
/// Hat die Skizze einen Ellipsenbogen (auch einen gelöschten Kurveneintrag, er steht in der
/// Datei)?
fn has_elliptic_arc(s: &Sketch) -> bool {
    s.curve_entries()
        .any(|k| matches!(k, CurveKind::EllipticArc { .. }))
}
/// Eine Zeichnung braucht Format 7, wenn sie ein Stücklistenblatt ist, Nummernkreise hat oder die
/// Stückliste auf dem Blatt eingeschaltet ist.
fn has_parts_data(d: &Drawing) -> bool {
    matches!(d.kind, DrawingKind::PartsList { .. })
        || !d.balloons.is_empty()
        || d.info.as_ref().is_some_and(|i| i.parts_list)
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
fn num(origin: &str, what: What, x: f64) -> Result<f64> {
    if x.is_finite() && x.abs() <= MAX_COORD_MM * 2.0 {
        Ok(x)
    } else {
        Err(ProjectError::corrupt(
            origin,
            tr!("proj-number-invalid", what = what()),
        ))
    }
}

/// Bezeichnung des Werts für Fehlermeldungen. Der Text entsteht erst im Fehlerfall, denn die
/// Prüfungen laufen für jede Zahl einer Datei.
type What<'a> = &'a dyn Fn() -> String;

/// Bezeichnung eines Feldes allein (ohne Namen), z. B. „Radius“.
type Label = fn() -> String;

/// `„Name“ Feld`, z. B. `„Quader 1“ Radius`.
fn field_label(name: &str, label: String) -> String {
    tr!("proj-field", name = name, label = label)
}

/// `Skizze „Name“ Feld`.
fn sketch_label(feature: &str, label: String) -> String {
    tr!(
        "proj-what-label",
        what = tr!("proj-w-sketch", name = feature),
        label = label
    )
}

/// `Zeichnung „Name“ Feld`.
fn drawing_label(drawing: &str, label: String) -> String {
    tr!(
        "proj-what-label",
        what = tr!("proj-w-drawing", name = drawing),
        label = label
    )
}

fn dv3(origin: &str, what: What, v: V3) -> Result<DVec3> {
    Ok(DVec3::new(
        num(origin, what, v[0])?,
        num(origin, what, v[1])?,
        num(origin, what, v[2])?,
    ))
}

fn check_len(origin: &str, what: What, n: usize) -> Result<()> {
    if n > MAX_LIST {
        Err(ProjectError::corrupt(
            origin,
            tr!("proj-too-many", what = what()),
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
        hint: dv3(origin, &|| field_label(name, tr!("proj-l-hint")), d.hinweis)?,
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
        hint: dv3(origin, &|| field_label(name, tr!("proj-l-hint")), d.hinweis)?,
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
            az,
            minor,
        } => CurveKindDto::Ellipse {
            mitte: center.index(),
            ax: *ax,
            ay: *ay,
            az: *az,
            klein: *minor,
        },
        CurveKind::Spline { points, handles } => CurveKindDto::Spline {
            punkte: points.iter().map(|p| p.index()).collect(),
            griffe: handles.iter().map(griff_dto).collect(),
        },
        CurveKind::EllipticArc {
            center,
            ax,
            ay,
            az,
            minor,
            start,
            end,
        } => CurveKindDto::Ellipsenbogen {
            mitte: center.index(),
            ax: *ax,
            ay: *ay,
            az: *az,
            klein: *minor,
            start: start.index(),
            ende: end.index(),
        },
        CurveKind::ControlSpline {
            points,
            degree,
            knots,
        } => CurveKindDto::KontrollSpline {
            punkte: points.iter().map(|p| p.index()).collect(),
            grad: *degree,
            knoten: knots.clone(),
        },
        CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        } => CurveKindDto::Kegelschnitt {
            start: start.index(),
            ende: end.index(),
            scheitel: apex.index(),
            rho: *rho,
        },
        CurveKind::FixedSpline { start, end, curve } => CurveKindDto::FesterSpline {
            start: start.index(),
            ende: end.index(),
            grad: curve.degree(),
            knoten: curve.knots().knots().to_vec(),
            punkte: curve.control_points().iter().map(|q| [q.x, q.y]).collect(),
            gewichte: curve.weights().to_vec(),
            // Höhen nur bei einer Freiformkurve im Raum (Format 20, Skizzen 10).
            hoehen: if curve.control_points().iter().any(|q| q.z != 0.0) {
                curve.control_points().iter().map(|q| q.z).collect()
            } else {
                Vec::new()
            },
        },
    }
}

fn griff_dto(h: &SplineHandle) -> GriffDto {
    GriffDto {
        an: h.at.index(),
        linie: h.line.index(),
        spitze: h.tip.index(),
        kruemmung: match h.curvature {
            HandleCurvature::Form => GriffKruemmungDto::Form,
            HandleCurvature::Straight => GriffKruemmungDto::Gerade,
            HandleCurvature::Arc { arc, center, other } => GriffKruemmungDto::Bogen {
                bogen: arc.index(),
                mitte: center.index(),
                anderes: other.index(),
            },
        },
    }
}

fn griff_from(d: &GriffDto) -> SplineHandle {
    let p = PointId::from_index;
    SplineHandle {
        at: p(d.an),
        line: CurveId::from_index(d.linie),
        tip: p(d.spitze),
        curvature: match d.kruemmung {
            GriffKruemmungDto::Form => HandleCurvature::Form,
            GriffKruemmungDto::Gerade => HandleCurvature::Straight,
            GriffKruemmungDto::Bogen {
                bogen,
                mitte,
                anderes,
            } => HandleCurvature::Arc {
                arc: CurveId::from_index(bogen),
                center: p(mitte),
                other: p(anderes),
            },
        },
    }
}

fn curve_from(d: &CurveKindDto) -> std::result::Result<CurveKind, String> {
    let p = PointId::from_index;
    Ok(match d {
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
            az,
            klein,
        } => CurveKind::Ellipse {
            center: p(*mitte),
            ax: *ax,
            ay: *ay,
            az: *az,
            minor: *klein,
        },
        CurveKindDto::Spline { punkte, griffe } => CurveKind::Spline {
            points: punkte.iter().map(|i| p(*i)).collect(),
            handles: griffe.iter().map(griff_from).collect(),
        },
        CurveKindDto::Ellipsenbogen {
            mitte,
            ax,
            ay,
            az,
            klein,
            start,
            ende,
        } => CurveKind::EllipticArc {
            center: p(*mitte),
            ax: *ax,
            ay: *ay,
            az: *az,
            minor: *klein,
            start: p(*start),
            end: p(*ende),
        },
        CurveKindDto::KontrollSpline {
            punkte,
            grad,
            knoten,
        } => CurveKind::ControlSpline {
            points: punkte.iter().map(|i| p(*i)).collect(),
            degree: *grad,
            knots: knoten.clone(),
        },
        CurveKindDto::Kegelschnitt {
            start,
            ende,
            scheitel,
            rho,
        } => CurveKind::Conic {
            start: p(*start),
            end: p(*ende),
            apex: p(*scheitel),
            rho: *rho,
        },
        CurveKindDto::FesterSpline {
            start,
            ende,
            grad,
            knoten,
            punkte,
            gewichte,
            hoehen,
        } => {
            // Ohne Höhen liegt die Kurve in der Skizzenebene; sonst je Kontrollpunkt eine.
            if !hoehen.is_empty() && hoehen.len() != punkte.len() {
                return Err(tr!("sketch-err-data-fixed-spline"));
            }
            let z = |i: usize| hoehen.get(i).copied().unwrap_or(0.0);
            CurveKind::FixedSpline {
                start: p(*start),
                end: p(*ende),
                curve: Arc::new(
                    NurbsCurve::new(
                        *grad,
                        knoten.clone(),
                        punkte
                            .iter()
                            .enumerate()
                            .map(|(i, q)| DVec3::new(q[0], q[1], z(i)))
                            .collect(),
                        gewichte.clone(),
                    )
                    .map_err(|e| e.to_string())?,
                ),
            }
        }
    })
}

/// Eintrag einer Abhängigkeit in der Datei. Alle Einträge kommen hinein, auch die internen
/// (`InPlane` und `EllipseAxes3` wie `ArcRadius`, ab Format 20): So behalten alle Abhängigkeiten
/// beim Laden ihre Ids.
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
        K::SplineCurvatureHandle(k, a) => D::KruemmungsGriff {
            kurve: c(k),
            punkt: p(a),
        },
        K::EllipticArcEnds(k) => D::EllipsenbogenEnden { kurve: c(k) },
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
        K::Collinear(a, b) => D::Kollinear { a: c(a), b: c(b) },
        K::Smooth(a, pa, b, pb) => D::Glatt {
            a: c(a),
            punkt_a: p(pa),
            b: c(b),
            punkt_b: p(pb),
        },
        K::EndTangent(a, pa, b, pb) => D::TangenteEnde {
            a: c(a),
            punkt_a: p(pa),
            b: c(b),
            punkt_b: p(pb),
        },
        K::DistanceX(a, b, v) => D::AbstandWaagerecht {
            a: p(a),
            b: p(b),
            wert: *v,
        },
        K::DistanceY(a, b, v) => D::AbstandSenkrecht {
            a: p(a),
            b: p(b),
            wert: *v,
        },
        K::CenterlineDiameter(a, k, v) => D::DurchmesserZurAchse {
            punkt: p(a),
            kurve: c(k),
            wert: *v,
        },
        K::SlotOverall(a, b, k, v) => D::LanglochGesamt {
            a: p(a),
            b: p(b),
            kappe: c(k),
            wert: *v,
        },
        K::PatternValue(m, slot, v) => D::MusterWert {
            muster: m.index(),
            feld: match slot {
                PatternSlot::Count1 => MusterFeldDto::Anzahl1,
                PatternSlot::Spacing1 => MusterFeldDto::Abstand1,
                PatternSlot::Count2 => MusterFeldDto::Anzahl2,
                PatternSlot::Spacing2 => MusterFeldDto::Abstand2,
                PatternSlot::Count => MusterFeldDto::Anzahl,
                PatternSlot::Angle => MusterFeldDto::Winkel,
            },
            wert: *v,
        },
        K::AlongZ(l) => D::EntlangZ { ziel: lop_to(l) },
        K::Fixed3(a, x, y, z) => D::Fest3 {
            punkt: p(a),
            x: *x,
            y: *y,
            z: *z,
        },
        K::InPlane(k) => D::InEbene { kurve: c(k) },
        K::EllipseAxes3(k, x, y, z, m) => D::Ellipsenachsen3 {
            kurve: c(k),
            x: *x,
            y: *y,
            z: *z,
            klein: *m,
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
        D::KruemmungsGriff { kurve, punkt } => K::SplineCurvatureHandle(c(*kurve), p(*punkt)),
        D::EllipsenbogenEnden { kurve } => K::EllipticArcEnds(c(*kurve)),
        D::EllipseGross { kurve, wert } => K::EllipseMajor(c(*kurve), *wert),
        D::EllipseKlein { kurve, wert } => K::EllipseMinor(c(*kurve), *wert),
        D::EllipseWinkel { kurve, wert } => K::EllipseAngle(c(*kurve), *wert),
        D::Kollinear { a, b } => K::Collinear(c(*a), c(*b)),
        D::Glatt {
            a,
            punkt_a,
            b,
            punkt_b,
        } => K::Smooth(c(*a), p(*punkt_a), c(*b), p(*punkt_b)),
        D::TangenteEnde {
            a,
            punkt_a,
            b,
            punkt_b,
        } => K::EndTangent(c(*a), p(*punkt_a), c(*b), p(*punkt_b)),
        D::AbstandWaagerecht { a, b, wert } => K::DistanceX(p(*a), p(*b), *wert),
        D::AbstandSenkrecht { a, b, wert } => K::DistanceY(p(*a), p(*b), *wert),
        D::DurchmesserZurAchse { punkt, kurve, wert } => {
            K::CenterlineDiameter(p(*punkt), c(*kurve), *wert)
        }
        D::LanglochGesamt { a, b, kappe, wert } => K::SlotOverall(p(*a), p(*b), c(*kappe), *wert),
        D::MusterWert { muster, feld, wert } => K::PatternValue(
            PatternId::from_index(*muster),
            match feld {
                MusterFeldDto::Anzahl1 => PatternSlot::Count1,
                MusterFeldDto::Abstand1 => PatternSlot::Spacing1,
                MusterFeldDto::Anzahl2 => PatternSlot::Count2,
                MusterFeldDto::Abstand2 => PatternSlot::Spacing2,
                MusterFeldDto::Anzahl => PatternSlot::Count,
                MusterFeldDto::Winkel => PatternSlot::Angle,
            },
            *wert,
        ),
        D::EntlangZ { ziel } => K::AlongZ(lop_from(ziel)),
        D::Fest3 { punkt, x, y, z } => K::Fixed3(p(*punkt), *x, *y, *z),
        D::InEbene { kurve } => K::InPlane(c(*kurve)),
        D::Ellipsenachsen3 {
            kurve,
            x,
            y,
            z,
            klein,
        } => K::EllipseAxes3(c(*kurve), *x, *y, *z, *klein),
    }
}

fn pattern_to(m: &PatternRecord) -> PatternDto {
    let dir = |d: PatternDir| match d {
        PatternDir::SketchX => MusterRichtungDto::SkizzeX,
        PatternDir::SketchY => MusterRichtungDto::SkizzeY,
        PatternDir::Line(l) => MusterRichtungDto::Linie { kurve: l.index() },
        PatternDir::Perpendicular => MusterRichtungDto::Senkrecht,
    };
    PatternDto {
        art: match m.kind {
            PatternKind::Rect {
                dirs,
                extent,
                symmetric,
            } => MusterArtDto::Rechteck {
                richtungen: dirs.map(dir),
                gesamt: extent,
                symmetrisch: symmetric,
            },
            PatternKind::Circular { center, mode } => MusterArtDto::Kreis {
                mitte: center.index(),
                art: match mode {
                    CircularMode::Full => KreisArtDto::Voll,
                    CircularMode::Angle => KreisArtDto::Winkel,
                    CircularMode::Symmetric => KreisArtDto::Symmetrisch,
                },
            },
        },
        vorlagen: m.sources.iter().map(|c| c.index()).collect(),
        vorlagenpunkte: m.source_points.iter().map(|p| p.index()).collect(),
        werte: m.values.iter().map(|c| c.index()).collect(),
        exemplare: m
            .instances
            .iter()
            .map(|i| ExemplarDto {
                lage: i.at,
                punkte: i.points.iter().map(|p| p.index()).collect(),
                kurven: i.curves.iter().map(|c| c.index()).collect(),
            })
            .collect(),
        abgewaehlt: m.suppressed.clone(),
        lebt: m.alive,
    }
}

/// Muster einer Skizze aus der Datei. Indizes werden hier gegen die Listen der Skizze geprüft
/// (`from_index` schnitte große Zahlen sonst ab); alles Weitere prüft `Sketch::from_data`.
fn patterns_from(origin: &str, feature: &str, d: &SketchDto) -> Result<Vec<PatternRecord>> {
    let missing = || {
        ProjectError::corrupt(
            origin,
            tr!(
                "proj-ctx-sketch",
                name = feature,
                msg = tr!("sketch-err-data-pattern-object")
            ),
        )
    };
    let (np, nc, nk) = (d.punkte.len(), d.kurven.len(), d.abhaengigkeiten.len());
    let point = |i: usize| {
        if i < np {
            Ok(PointId::from_index(i))
        } else {
            Err(missing())
        }
    };
    let curve = |i: usize| {
        if i < nc {
            Ok(CurveId::from_index(i))
        } else {
            Err(missing())
        }
    };
    let dir = |r: MusterRichtungDto| -> Result<PatternDir> {
        Ok(match r {
            MusterRichtungDto::SkizzeX => PatternDir::SketchX,
            MusterRichtungDto::SkizzeY => PatternDir::SketchY,
            MusterRichtungDto::Linie { kurve } => PatternDir::Line(curve(kurve)?),
            MusterRichtungDto::Senkrecht => PatternDir::Perpendicular,
        })
    };
    check_len(origin, &|| tr!("proj-l-patterns"), d.muster.len())?;
    let mut out = Vec::with_capacity(d.muster.len());
    for m in &d.muster {
        check_len(origin, &|| tr!("proj-l-instances"), m.exemplare.len())?;
        check_len(origin, &|| tr!("proj-l-instances"), m.abgewaehlt.len())?;
        check_len(origin, &|| tr!("proj-l-curves"), m.vorlagen.len())?;
        check_len(origin, &|| tr!("proj-l-points"), m.vorlagenpunkte.len())?;
        check_len(origin, &|| tr!("proj-l-constraints"), m.werte.len())?;
        for e in &m.exemplare {
            check_len(origin, &|| tr!("proj-l-points"), e.punkte.len())?;
            check_len(origin, &|| tr!("proj-l-curves"), e.kurven.len())?;
        }
        let kind = match m.art {
            MusterArtDto::Rechteck {
                richtungen,
                gesamt,
                symmetrisch,
            } => PatternKind::Rect {
                dirs: [dir(richtungen[0])?, dir(richtungen[1])?],
                extent: gesamt,
                symmetric: symmetrisch,
            },
            MusterArtDto::Kreis { mitte, art } => PatternKind::Circular {
                center: point(mitte)?,
                mode: match art {
                    KreisArtDto::Voll => CircularMode::Full,
                    KreisArtDto::Winkel => CircularMode::Angle,
                    KreisArtDto::Symmetrisch => CircularMode::Symmetric,
                },
            },
        };
        let values = m
            .werte
            .iter()
            .map(|&i| {
                if i < nk {
                    Ok(ConstraintId::from_index(i))
                } else {
                    Err(missing())
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let mut instances = Vec::with_capacity(m.exemplare.len());
        for e in &m.exemplare {
            instances.push(PatternInstance {
                at: e.lage,
                points: e.punkte.iter().map(|&i| point(i)).collect::<Result<_>>()?,
                curves: e.kurven.iter().map(|&i| curve(i)).collect::<Result<_>>()?,
            });
        }
        out.push(PatternRecord {
            kind,
            sources: m
                .vorlagen
                .iter()
                .map(|&i| curve(i))
                .collect::<Result<_>>()?,
            source_points: m
                .vorlagenpunkte
                .iter()
                .map(|&i| point(i))
                .collect::<Result<_>>()?,
            values,
            instances,
            suppressed: m.abgewaehlt.clone(),
            alive: m.lebt,
        });
    }
    Ok(out)
}

fn h_align_to(h: HAlign) -> WaagerechtDto {
    match h {
        HAlign::Left => WaagerechtDto::Links,
        HAlign::Center => WaagerechtDto::Mitte,
        HAlign::Right => WaagerechtDto::Rechts,
    }
}

fn h_align_from(h: WaagerechtDto) -> HAlign {
    match h {
        WaagerechtDto::Links => HAlign::Left,
        WaagerechtDto::Mitte => HAlign::Center,
        WaagerechtDto::Rechts => HAlign::Right,
    }
}

fn text_to(t: &TextRecord) -> TextDto {
    TextDto {
        inhalt: t.content.clone(),
        schrift: t.style.font.clone(),
        fett: t.style.bold,
        kursiv: t.style.italic,
        hoehe: t.height,
        abstand: t.style.spacing,
        umkehren_waagerecht: t.flip_h,
        umkehren_senkrecht: t.flip_v,
        lage: match t.placement {
            TextPlacement::Box {
                lines,
                corners,
                h,
                v,
            } => TextLageDto::Rahmen {
                linien: lines.map(|c| c.index()),
                ecken: corners.map(|p| p.index()),
                waagerecht: h_align_to(h),
                senkrecht: match v {
                    VAlign::Top => SenkrechtDto::Oben,
                    VAlign::Middle => SenkrechtDto::Mitte,
                    VAlign::Bottom => SenkrechtDto::Unten,
                },
            },
            TextPlacement::Path {
                path,
                above,
                fit,
                h,
            } => TextLageDto::Pfad {
                kurve: path.index(),
                oberhalb: above,
                anpassen: fit,
                waagerecht: h_align_to(h),
            },
        },
        normale: t.normal.map(v3),
        kurven: t.curves.iter().map(|c| c.index()).collect(),
        punkte: t.points.iter().map(|p| p.index()).collect(),
        lebt: t.alive,
    }
}

/// Texte einer Skizze aus der Datei (Skizzen 8). Indizes werden hier gegen die Listen der Skizze
/// geprüft (`from_index` schnitte große Zahlen sonst ab); Werte, Rahmen, Pfad und Zuordnung prüft
/// `Sketch::from_data` (Spec G5). Das gesetzte `TextShape` fehlt: Geladene Texte sind „noch nicht
/// gesetzt“ (Spec G4).
fn texts_from(origin: &str, feature: &str, d: &SketchDto) -> Result<Vec<TextRecord>> {
    let missing = || {
        ProjectError::corrupt(
            origin,
            tr!(
                "proj-ctx-sketch",
                name = feature,
                msg = tr!(
                    "sketch-err-text-data",
                    reason = tr!("sketch-err-text-data-object")
                )
            ),
        )
    };
    let (np, nc) = (d.punkte.len(), d.kurven.len());
    let point = |i: usize| {
        if i < np {
            Ok(PointId::from_index(i))
        } else {
            Err(missing())
        }
    };
    let curve = |i: usize| {
        if i < nc {
            Ok(CurveId::from_index(i))
        } else {
            Err(missing())
        }
    };
    check_len(origin, &|| tr!("proj-l-texts"), d.texte.len())?;
    let mut out = Vec::with_capacity(d.texte.len());
    for t in &d.texte {
        check_len(origin, &|| tr!("proj-l-curves"), t.kurven.len())?;
        check_len(origin, &|| tr!("proj-l-points"), t.punkte.len())?;
        let placement = match t.lage {
            TextLageDto::Rahmen {
                linien,
                ecken,
                waagerecht,
                senkrecht,
            } => TextPlacement::Box {
                lines: [
                    curve(linien[0])?,
                    curve(linien[1])?,
                    curve(linien[2])?,
                    curve(linien[3])?,
                ],
                corners: [
                    point(ecken[0])?,
                    point(ecken[1])?,
                    point(ecken[2])?,
                    point(ecken[3])?,
                ],
                h: h_align_from(waagerecht),
                v: match senkrecht {
                    SenkrechtDto::Oben => VAlign::Top,
                    SenkrechtDto::Mitte => VAlign::Middle,
                    SenkrechtDto::Unten => VAlign::Bottom,
                },
            },
            TextLageDto::Pfad {
                kurve,
                oberhalb,
                anpassen,
                waagerecht,
            } => TextPlacement::Path {
                path: curve(kurve)?,
                above: oberhalb,
                fit: anpassen,
                h: h_align_from(waagerecht),
            },
        };
        out.push(TextRecord {
            content: t.inhalt.clone(),
            style: TextStyle {
                font: t.schrift.clone(),
                bold: t.fett,
                italic: t.kursiv,
                spacing: t.abstand,
            },
            height: t.hoehe,
            flip_h: t.umkehren_waagerecht,
            flip_v: t.umkehren_senkrecht,
            placement,
            // Normiert und in der Ebene prüft `Sketch::from_data`.
            normal: t
                .normale
                .map(|n| dv3(origin, &|| sketch_label(feature, tr!("proj-l-normal")), n))
                .transpose()?,
            curves: t.kurven.iter().map(|&i| curve(i)).collect::<Result<_>>()?,
            points: t.punkte.iter().map(|&i| point(i)).collect::<Result<_>>()?,
            alive: t.lebt,
        });
    }
    Ok(out)
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
                z: p.z,
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
                mittellinie: c.centerline,
                aktiv: c.alive,
                rahmen: c.show_frame,
                normale: c.normal.map(v3),
                kamm: c.comb.map(|k| KammDto {
                    dichte: k.density,
                    massstab: k.scale,
                }),
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
                gegenwinkel: c.supplement,
            })
            .collect(),
        muster: d.patterns.iter().map(pattern_to).collect(),
        texte: d.texts.iter().map(text_to).collect(),
        drei_d: d.three_d,
    }
}

/// Liest eine Skizze; dazu die Zahl der Muster, die beim Laden aufgelöst wurden, weil sie in einer
/// Schleife lagen (`Sketch::from_data_counted`, Skizzen 5).
fn sketch_from(origin: &str, feature: &str, d: &SketchDto) -> Result<(Sketch, usize)> {
    let ctx =
        |m: String| ProjectError::corrupt(origin, tr!("proj-ctx-sketch", name = feature, msg = m));
    check_len(origin, &|| tr!("proj-l-values"), d.werte.len())?;
    check_len(origin, &|| tr!("proj-l-points"), d.punkte.len())?;
    check_len(origin, &|| tr!("proj-l-curves"), d.kurven.len())?;
    check_len(
        origin,
        &|| tr!("proj-l-constraints"),
        d.abhaengigkeiten.len(),
    )?;
    let data = SketchData {
        name: d.name.clone(),
        frame: Frame {
            origin: dv3(
                origin,
                &|| sketch_label(feature, tr!("proj-l-origin")),
                d.ebene.ursprung,
            )?,
            x: dv3(
                origin,
                &|| sketch_label(feature, tr!("proj-l-x-axis")),
                d.ebene.x,
            )?,
            y: dv3(
                origin,
                &|| sketch_label(feature, tr!("proj-l-y-axis")),
                d.ebene.y,
            )?,
            z: dv3(
                origin,
                &|| sketch_label(feature, tr!("proj-l-z-axis")),
                d.ebene.z,
            )?,
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
                z: p.z,
                construction: p.konstruktion,
                alive: p.aktiv,
            })
            .collect(),
        curves: d
            .kurven
            .iter()
            .map(|c| {
                Ok(CurveRecord {
                    kind: curve_from(&c.art).map_err(ctx)?,
                    construction: c.konstruktion,
                    centerline: c.mittellinie,
                    show_frame: c.rahmen,
                    // Normiert prüft `Sketch::from_data`.
                    normal: c
                        .normale
                        .map(|n| dv3(origin, &|| sketch_label(feature, tr!("proj-l-normal")), n))
                        .transpose()?,
                    // Werte und Kurvenart prüft `Sketch::from_data`.
                    comb: c.kamm.map(|k| CombSettings {
                        density: k.dichte,
                        scale: k.massstab,
                    }),
                    alive: c.aktiv,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        constraints: d
            .abhaengigkeiten
            .iter()
            .map(|c| ConstraintRecord {
                kind: constraint_from(&c.art),
                branch: c.zweig,
                supplement: c.gegenwinkel,
                driven: c.gesteuert,
                enabled: c.aktiv,
                internal: c.intern,
                extra: c.extra,
                alive: c.lebt,
            })
            .collect(),
        patterns: patterns_from(origin, feature, d)?,
        texts: texts_from(origin, feature, d)?,
        three_d: d.drei_d,
    };
    Sketch::from_data_counted(data).map_err(|e| ctx(e.to_string()))
}

// ---- Feature-Arten ----------------------------------------------------------------------

fn plane_name(p: StandardPlane) -> &'static str {
    match p {
        StandardPlane::XY => "xy",
        StandardPlane::XZ => "xz",
        StandardPlane::YZ => "yz",
    }
}

/// Profilfelder einer Extrusion oder Rotation in der Datei: genau eins von `profil` (alte
/// Profilnummer) und `profile` (Verweise, ab Format 13); bei anderen Features beide leer.
#[derive(Clone, Debug, Default)]
struct ProfileField {
    profil: Option<usize>,
    profile: Option<Vec<ProfilVerweisDto>>,
}

/// Übergang alter Profilnummern je Skizzen-Feature, für einen Lade- oder Speichervorgang einmal
/// berechnet (die Profile einer Skizze werden nicht je Feature neu bestimmt). Verwendet wird
/// immer die gespeicherte Skizze des Skizzen-Features, beim Laden wie beim Speichern.
struct Migrations<'a> {
    sketches: HashMap<FeatureId, &'a Sketch>,
    cache: HashMap<FeatureId, LegacyMigration>,
}

impl<'a> Migrations<'a> {
    fn new(features: &'a [SnapshotFeature]) -> Self {
        let sketches = features
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::Sketch(sf) => Some((f.id, &sf.sketch)),
                _ => None,
            })
            .collect();
        Self {
            sketches,
            cache: HashMap::new(),
        }
    }

    /// Übergang der Skizze `id`; `None`, wenn `id` kein Skizzen-Feature ist.
    fn of(&mut self, id: FeatureId) -> Option<&LegacyMigration> {
        let sketch = *self.sketches.get(&id)?;
        Some(
            self.cache
                .entry(id)
                .or_insert_with(|| sketch.legacy_migration()),
        )
    }

    /// Profilfelder für die gewählten Profile `sel` der Skizze `sketch`: `Index(n)` als
    /// `profil: n`; Verweise, die heute dieselben Flächen wählen wie ein altes Profil `n`, auch
    /// als `profil: n` (die Datei bleibt auf ihrem Format); sonst als `profile`.
    fn field(&mut self, sketch: FeatureId, sel: &ProfileSel) -> ProfileField {
        let number = |n| ProfileField {
            profil: Some(n),
            profile: None,
        };
        match sel {
            ProfileSel::Index(n) => number(*n),
            ProfileSel::Refs(refs) => match self.of(sketch).and_then(|m| m.legacy_index_of(refs)) {
                Some(n) => number(n),
                None => ProfileField {
                    profil: None,
                    profile: Some(refs.iter().map(profile_ref_to).collect()),
                },
            },
        }
    }
}

/// Profilfelder jedes Features des Stands (in derselben Reihenfolge).
fn profile_fields(s: &Snapshot) -> Vec<ProfileField> {
    let mut migrations = Migrations::new(&s.features);
    s.features
        .iter()
        .map(|f| match &f.kind {
            FeatureKind::Extrude {
                sketch, profiles, ..
            }
            | FeatureKind::Revolve {
                sketch, profiles, ..
            } => migrations.field(*sketch, profiles),
            _ => ProfileField::default(),
        })
        .collect()
}

fn piece_to(p: &PieceRef) -> StueckDto {
    (p.curve.index(), p.u0, p.u1, p.left, p.len)
}

fn profile_ref_to(r: &ProfileRef) -> ProfilVerweisDto {
    ProfilVerweisDto {
        aussen: r.outer.iter().map(piece_to).collect(),
        loecher: r.holes.iter().map(piece_to).collect(),
    }
}

/// Gewählte Profile aus den Feldern `profil`/`profile` (genau eins muss da sein). Geprüft werden
/// hier die Anzahl der Stücke und ihre Zahlen (endlich, `u0 < u1 ≤ u0 + 1`, Länge nicht
/// negativ); die Kurven-Indizes prüft [`migrate_profiles`] gegen die Skizze.
fn profile_sel_from(
    origin: &str,
    name: &str,
    profil: Option<usize>,
    profile: Option<&[ProfilVerweisDto]>,
) -> Result<ProfileSel> {
    let err = |msg: String| {
        ProjectError::corrupt(origin, tr!("proj-ctx-feature", name = name, msg = msg))
    };
    let list = match (profil, profile) {
        (Some(n), None) => return Ok(ProfileSel::Index(n)),
        (None, Some(list)) => list,
        _ => return Err(err(tr!("proj-err-profile-field"))),
    };
    let pieces = list.iter().fold(list.len(), |sum, r| {
        sum.saturating_add(r.aussen.len())
            .saturating_add(r.loecher.len())
    });
    check_len(origin, &|| tr!("proj-l-profile-pieces"), pieces)?;
    let piece = |&(curve, u0, u1, left, len): &StueckDto| -> Result<PieceRef> {
        let ok = u0.is_finite()
            && u1.is_finite()
            && len.is_finite()
            && u0 < u1
            // Dateien vor der Nachbesserung schrieben einen vollen Umlauf mitunter ein ulp zu lang
            && u1 <= u0 + 1.0 + 1e-9
            && len >= 0.0;
        if !ok {
            return Err(err(tr!("proj-profile-piece-invalid")));
        }
        Ok(PieceRef {
            curve: CurveId::from_index(curve),
            u0,
            u1: u1.min(u0 + 1.0),
            left,
            len,
        })
    };
    let refs = list
        .iter()
        .map(|r| {
            Ok(ProfileRef {
                outer: r.aussen.iter().map(piece).collect::<Result<_>>()?,
                holes: r.loecher.iter().map(piece).collect::<Result<_>>()?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(ProfileSel::Refs(refs))
}

/// Nach dem Lesen aller Features: Verweise müssen Kurven ihrer (gespeicherten) Skizze nennen;
/// alte Profilnummern `Index(n)` werden auf Verweise der heutigen Flächen umgerechnet
/// ([`LegacyMigration::resolvable`]). Lässt sich ein altes Profil nicht umrechnen, bleibt
/// `Index(n)` (beim Berechnen gilt dann das alte Profil selbst).
fn migrate_profiles(origin: &str, features: &mut [SnapshotFeature]) -> Result<()> {
    let mut replace: Vec<(usize, ProfileSel)> = Vec::new();
    let mut migrations = Migrations::new(features);
    for (i, f) in features.iter().enumerate() {
        let (FeatureKind::Extrude {
            sketch, profiles, ..
        }
        | FeatureKind::Revolve {
            sketch, profiles, ..
        }) = &f.kind
        else {
            continue;
        };
        match profiles {
            ProfileSel::Refs(refs) => {
                // Ohne Skizze meldet `Document::from_snapshot` den Fehler.
                let Some(sk) = migrations.sketches.get(sketch) else {
                    continue;
                };
                let count = sk.curve_entries().count();
                if let Some(p) = refs
                    .iter()
                    .flat_map(|r| r.outer.iter().chain(&r.holes))
                    .find(|p| p.curve.index() >= count)
                {
                    return Err(ProjectError::corrupt(
                        origin,
                        tr!(
                            "proj-ctx-feature",
                            name = f.name.as_str(),
                            msg = tr!(
                                "proj-profile-curve-unknown",
                                curve = p.curve.index().to_string()
                            )
                        ),
                    ));
                }
            }
            ProfileSel::Index(n) => {
                if let Some(refs) = migrations.of(*sketch).and_then(|m| m.resolvable(*n)) {
                    replace.push((i, ProfileSel::Refs(refs.to_vec())));
                }
            }
        }
    }
    drop(migrations);
    for (i, sel) in replace {
        if let FeatureKind::Extrude { profiles, .. } | FeatureKind::Revolve { profiles, .. } =
            &mut features[i].kind
        {
            *profiles = sel;
        }
    }
    Ok(())
}

/// Art eines Features für die Datei; `forms` ordnet jeder Formkennung des Dokuments die
/// Kennung zu, unter der ihr Text in der Datei steht (siehe [`step_forms_to`]); `profile` sind
/// die Profilfelder einer Extrusion oder Rotation ([`profile_fields`]).
fn kind_to(
    k: &FeatureKind,
    forms: &BTreeMap<StepShapeId, u64>,
    profile: ProfileField,
) -> Result<KindDto> {
    Ok(match k {
        FeatureKind::Sketch(s) => KindDto::Skizze(Box::new(SketchFeatureDto {
            skizze: sketch_to(&s.sketch),
            lage: base_to(&match &s.plane {
                SketchPlane::Standard(p) => PlaneBase::Standard(*p),
                SketchPlane::OnFace(r) => PlaneBase::Face(r.clone()),
                SketchPlane::OnPlane(id) => PlaneBase::Plane(*id),
            }),
            kopien: s.copies.iter().map(copy_to).collect(),
            formeln: s
                .formulas
                .iter()
                .map(|(id, text)| SketchFormulaDto {
                    abhaengigkeit: id.index(),
                    formel: text.clone(),
                })
                .collect(),
            hoehenformeln: s
                .text_formulas
                .iter()
                .map(|(id, text)| TextFormulaDto {
                    text: id.index(),
                    formel: text.clone(),
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
        FeatureKind::Extrude { sketch, height, .. } => KindDto::Extrusion {
            skizze: sketch.0,
            profil: profile.profil,
            profile: profile.profile,
            hoehe: *height,
        },
        FeatureKind::Revolve { sketch, axis, .. } => KindDto::Rotation {
            skizze: sketch.0,
            profil: profile.profil,
            profile: profile.profile,
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
        FeatureKind::StepBody { shape } => KindDto::StepKoerper {
            // Fehlt die Form im Bild, bleibt die Kennung; das Selbstlesen meldet sie dann.
            form: forms.get(shape).copied().unwrap_or(shape.0),
        },
        FeatureKind::Plane(p) => KindDto::Konstruktionsebene(Box::new(plane_to(p))),
        FeatureKind::Canvas(c) => KindDto::Leinwand(Box::new(canvas_to(c))),
    })
}

// ---- Leinwände und Bilder (Format 19) ---------------------------------------------------

fn canvas_to(c: &CanvasFeature) -> LeinwandDto {
    LeinwandDto {
        bild: c.image.0,
        lage: base_to(&match &c.base {
            CanvasBase::Standard(p) => PlaneBase::Standard(*p),
            CanvasBase::Face(r) => PlaneBase::Face(r.clone()),
            CanvasBase::Plane(id) => PlaneBase::Plane(*id),
        }),
        versatz: c.offset.to_array(),
        winkel: c.angle,
        massstab: c.scale.to_array(),
        spiegeln_waagerecht: c.flip_h,
        spiegeln_senkrecht: c.flip_v,
        deckkraft: c.opacity,
        durch_objekte: c.display_through,
        auswaehlbar: c.selectable,
        letzte_lage: c.last_base.as_ref().map(frame_to),
    }
}

/// Liest eine Leinwand. Zahlen werden wie überall geprüft (endlich, im Rechenbereich); ob
/// Maßstab, Winkel und Deckkraft passen, prüft das Dokument beim Rechnen (wie bei Netz-Körpern:
/// Die Leinwand bekommt dann den Fehlerstatus).
fn canvas_from(origin: &str, name: &str, d: &LeinwandDto) -> Result<CanvasFeature> {
    let n = |label: Label, x: f64| num(origin, &|| field_label(name, label()), x);
    let v = |label: Label, x: V3| dv3(origin, &|| field_label(name, label()), x);
    let last_base = match &d.letzte_lage {
        Some(f) => Some(Frame {
            origin: v(|| tr!("proj-l-origin"), f.ursprung)?,
            x: v(|| tr!("proj-l-x-axis"), f.x)?,
            y: v(|| tr!("proj-l-y-axis"), f.y)?,
            z: v(|| tr!("proj-l-z-axis"), f.z)?,
        }),
        None => None,
    };
    Ok(CanvasFeature {
        image: ImageId(d.bild),
        base: match base_from(origin, name, &d.lage)? {
            PlaneBase::Standard(p) => CanvasBase::Standard(p),
            PlaneBase::Face(r) => CanvasBase::Face(r),
            PlaneBase::Plane(id) => CanvasBase::Plane(id),
        },
        offset: DVec2::new(
            n(|| tr!("proj-l-position"), d.versatz[0])?,
            n(|| tr!("proj-l-position"), d.versatz[1])?,
        ),
        angle: n(|| tr!("proj-l-rotation"), d.winkel)?,
        scale: DVec2::new(
            n(|| tr!("proj-l-scale"), d.massstab[0])?,
            n(|| tr!("proj-l-scale"), d.massstab[1])?,
        ),
        flip_h: d.spiegeln_waagerecht,
        flip_v: d.spiegeln_senkrecht,
        opacity: d.deckkraft,
        display_through: d.durch_objekte,
        selectable: d.auswaehlbar,
        last_base,
    })
}

fn image_format_name(f: ImageFormat) -> &'static str {
    match f {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Tiff => "tiff",
    }
}

/// Bilder für die Datei, aufsteigend nach Kennung. Vorher die Gesamtgrenze, damit ein zu großes
/// Dokument eine klare Meldung bekommt und nicht erst riesige Texte entstehen.
fn images_to(s: &Snapshot) -> Result<Vec<ImageDto>> {
    images_to_limited(s, MAX_IMAGES_BYTES)
}

/// Wie [`images_to`] mit eigener Gesamtgrenze (Tests ohne riesige Bilder).
fn images_to_limited(s: &Snapshot, max: u64) -> Result<Vec<ImageDto>> {
    let total = s
        .images
        .values()
        .fold(0u64, |sum, i| sum.saturating_add(i.bytes.len() as u64));
    if total > max {
        let mb = |b: u64| (b / (1024 * 1024)).to_string();
        return Err(ProjectError::Invalid(tr!(
            "proj-images-too-large-to-save",
            size = mb(total),
            max = mb(max)
        )));
    }
    Ok(s.images
        .iter()
        .map(|(id, i)| ImageDto {
            id: id.0,
            format: image_format_name(i.format).into(),
            daten: base64::encode(&i.bytes),
        })
        .collect())
}

/// Liest die Bilder einer Datei. Vor dem Dekodieren wird die Länge (einzeln und in Summe)
/// gegen die Grenzen geprüft; danach prüft `probe_image` Format, Inhalt und Kantenlänge. Maße
/// und Prüfsumme kommen aus den Bytes, nicht aus der Datei.
fn images_from(origin: &str, list: &[ImageDto]) -> Result<BTreeMap<ImageId, Arc<CanvasImage>>> {
    images_from_limited(origin, list, MAX_IMAGES_BYTES)
}

/// Wie [`images_from`] mit eigener Gesamtgrenze (Tests ohne riesige Bilder).
fn images_from_limited(
    origin: &str,
    list: &[ImageDto],
    max: u64,
) -> Result<BTreeMap<ImageId, Arc<CanvasImage>>> {
    let mut total: u64 = 0;
    let mut out = BTreeMap::new();
    for d in list {
        let bad = |m: &str| {
            ProjectError::corrupt(
                origin,
                tr!("proj-ctx-image", id = d.id.to_string(), msg = m),
            )
        };
        if d.id == 0 {
            return Err(bad(&tr!("proj-bad-id")));
        }
        let format = match d.format.as_str() {
            "png" => ImageFormat::Png,
            "jpeg" => ImageFormat::Jpeg,
            "tiff" => ImageFormat::Tiff,
            other => {
                return Err(bad(&tr!(
                    "proj-unknown-image-format",
                    value = quoted(other)
                )));
            }
        };
        let len = base64::decoded_len(&d.daten).map_err(|e| bad(&e.to_string()))? as u64;
        if len > MAX_IMAGE_BYTES {
            return Err(bad(&tr!("proj-image-too-big")));
        }
        total = total.saturating_add(len);
        if total > max {
            return Err(bad(&tr!("proj-images-too-big")));
        }
        let bytes = base64::decode(&d.daten).map_err(|e| bad(&e.to_string()))?;
        // Die Endung zum Format in der Datei: `probe_image` lehnt ab, wenn der Inhalt ein
        // anderes Format ist.
        let info = probe_image(image_format_name(format), &bytes).map_err(|e| match e {
            ExchangeError::ImageWrongExtension => bad(&tr!(
                "proj-image-format-mismatch",
                format = image_format_name(format)
            )),
            other => bad(&other.to_string()),
        })?;
        let img = CanvasImage {
            hash: image_hash(&bytes),
            bytes: Arc::from(bytes),
            format: info.format,
            width: info.width,
            height: info.height,
        };
        if out.insert(ImageId(d.id), Arc::new(img)).is_some() {
            return Err(bad(&tr!("proj-duplicate-id")));
        }
    }
    Ok(out)
}

// ---- Ebenen und Projektionen (Format 16) ------------------------------------------------

fn base_to(b: &PlaneBase) -> PlaneDto {
    match b {
        PlaneBase::Standard(p) => PlaneDto::Standard {
            ebene: plane_name(*p).into(),
        },
        PlaneBase::Face(r) => PlaneDto::Flaeche {
            flaeche: face_to(r),
        },
        PlaneBase::Plane(id) => PlaneDto::Ebene { feature: id.0 },
    }
}

/// Fehler „beschädigt“ mit dem Feature `name` im Text.
fn feature_corrupt(origin: &str, name: &str, msg: String) -> ProjectError {
    ProjectError::corrupt(origin, tr!("proj-ctx-feature", name = name, msg = msg))
}

fn base_from(origin: &str, name: &str, d: &PlaneDto) -> Result<PlaneBase> {
    Ok(match d {
        PlaneDto::Standard { ebene } => PlaneBase::Standard(match ebene.as_str() {
            "xy" => StandardPlane::XY,
            "xz" => StandardPlane::XZ,
            "yz" => StandardPlane::YZ,
            other => {
                return Err(feature_corrupt(
                    origin,
                    name,
                    tr!("proj-unknown-plane", value = other),
                ));
            }
        }),
        PlaneDto::Flaeche { flaeche } => PlaneBase::Face(face_from(origin, name, flaeche)?),
        PlaneDto::Ebene { feature } => PlaneBase::Plane(FeatureId(*feature)),
    })
}

fn copy_to(c: &EdgeCopy) -> EdgeCopyDto {
    let mut d = EdgeCopyDto {
        kurve: c.curve().map(|id| id.index()),
        punkt: c.point().map(|id| id.index()),
        stueck: c.piece,
        stuecke: c.of,
        kante: None,
        schnitt: None,
        silhouette: None,
        einbezug: None,
        skizzenkurve: None,
        auf_flaeche: None,
        schnittkurve: None,
        drehprofil: None,
        isokurve: None,
    };
    match &c.source {
        CopySource::Edge(e) => d.kante = Some(edge_to(e)),
        CopySource::Section { face, index } => {
            d.schnitt = Some(FaceIndexDto {
                flaeche: face_to(face),
                index: *index,
            });
        }
        CopySource::Silhouette { face, index } => {
            d.silhouette = Some(FaceIndexDto {
                flaeche: face_to(face),
                index: *index,
            });
        }
        CopySource::Include(r) => d.einbezug = Some(include_to(r)),
        CopySource::SketchCurve { sketch, curve } => {
            d.skizzenkurve = Some(SkizzenkurveDto {
                skizze: sketch.0,
                kurve: curve.index(),
            });
        }
        CopySource::OnSurface(p) => d.auf_flaeche = Some(on_surface_to(p)),
        CopySource::Intersection(p) => d.schnittkurve = Some(intersection_to(p)),
        CopySource::SpunProfile(p) => d.drehprofil = Some(spun_to(p)),
        CopySource::Isoparametric {
            face,
            direction,
            value,
        } => {
            d.isokurve = Some(IsokurveDto {
                flaeche: face_to(face),
                richtung: match direction {
                    IsoDirection::U => "u",
                    IsoDirection::V => "v",
                }
                .into(),
                anteil: *value,
            });
        }
    }
    d
}

fn axis_ref_to(a: &AxisRef) -> AchseDto {
    match a {
        AxisRef::Standard(a) => AchseDto::Standard {
            achse: axis_name(*a).into(),
        },
        AxisRef::Edge(e) => AchseDto::Kante { kante: edge_to(e) },
        AxisRef::SketchLine { sketch, curve } => AchseDto::Skizzenlinie {
            skizze: sketch.0,
            kurve: curve.index(),
        },
    }
}

fn axis_ref_from(origin: &str, name: &str, d: &AchseDto) -> Result<AxisRef> {
    Ok(match d {
        AchseDto::Standard { achse } => AxisRef::Standard(match achse.as_str() {
            "x" => StdAxis::X,
            "y" => StdAxis::Y,
            "z" => StdAxis::Z,
            other => {
                return Err(feature_corrupt(
                    origin,
                    name,
                    tr!("proj-unknown-axis", value = quoted(other)),
                ));
            }
        }),
        AchseDto::Kante { kante } => AxisRef::Edge(edge_from(origin, name, kante)?),
        AchseDto::Skizzenlinie { skizze, kurve } => AxisRef::SketchLine {
            sketch: FeatureId(*skizze),
            curve: CurveId::from_index(sketch_index(origin, name, *kurve)?),
        },
    })
}

fn spun_to(p: &SpunSpec) -> DrehprofilDto {
    DrehprofilDto {
        auswahl: match &p.selection {
            SpunSelection::Bodies(b) => DrehauswahlDto::Koerper {
                koerper: b.iter().map(body_to).collect(),
            },
            SpunSelection::Elements(e) => DrehauswahlDto::Elemente {
                elemente: e.iter().map(curve_source_to).collect(),
            },
        },
        achse: match &p.axis {
            SpunAxis::Own(c) => DrehachseDto::Eigene { kurve: c.index() },
            SpunAxis::Ref(a) => DrehachseDto::Bezug {
                achse: axis_ref_to(a),
            },
        },
        toleranz: p.tolerance,
        umkehren: p.flip,
    }
}

/// Liest ein Drehprofil (ab Format 22); die Toleranz muss positiv und endlich sein.
fn spun_from(origin: &str, name: &str, d: &DrehprofilDto) -> Result<SpunSpec> {
    let selection = match &d.auswahl {
        DrehauswahlDto::Koerper { koerper } => {
            check_len(origin, &|| tr!("proj-l-edge-copies"), koerper.len())?;
            SpunSelection::Bodies(koerper.iter().map(body_from).collect())
        }
        DrehauswahlDto::Elemente { elemente } => {
            check_len(origin, &|| tr!("proj-l-edge-copies"), elemente.len())?;
            SpunSelection::Elements(
                elemente
                    .iter()
                    .map(|s| curve_source_from(origin, name, s))
                    .collect::<Result<Vec<_>>>()?,
            )
        }
    };
    let axis = match &d.achse {
        DrehachseDto::Eigene { kurve } => {
            SpunAxis::Own(CurveId::from_index(sketch_index(origin, name, *kurve)?))
        }
        DrehachseDto::Bezug { achse } => SpunAxis::Ref(axis_ref_from(origin, name, achse)?),
    };
    let tolerance = num(
        origin,
        &|| field_label(name, tr!("proj-l-tolerance")),
        d.toleranz,
    )?;
    if tolerance <= 0.0 {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-number-invalid",
                what = field_label(name, tr!("proj-l-tolerance"))
            ),
        ));
    }
    Ok(SpunSpec {
        selection,
        axis,
        tolerance,
        flip: d.umkehren,
    })
}

/// Liest eine Isokurve (ab Format 22): Richtung `u` oder `v`, Anteil endlich in 0–1.
fn iso_from(origin: &str, name: &str, d: &IsokurveDto) -> Result<CopySource> {
    let direction = match d.richtung.as_str() {
        "u" => IsoDirection::U,
        "v" => IsoDirection::V,
        other => {
            return Err(feature_corrupt(
                origin,
                name,
                tr!("proj-unknown-iso-direction", value = quoted(other)),
            ));
        }
    };
    if !(d.anteil.is_finite() && (0.0..=1.0).contains(&d.anteil)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-number-invalid",
                what = field_label(name, tr!("proj-l-iso-share"))
            ),
        ));
    }
    Ok(CopySource::Isoparametric {
        face: face_from(origin, name, &d.flaeche)?,
        direction,
        value: d.anteil,
    })
}

fn axis_name(a: StdAxis) -> &'static str {
    match a {
        StdAxis::X => "x",
        StdAxis::Y => "y",
        StdAxis::Z => "z",
    }
}

fn include_to(r: &IncludeRef) -> EinbezugDto {
    match r {
        IncludeRef::Edge(e) => EinbezugDto::Kante { kante: edge_to(e) },
        IncludeRef::FaceBoundary(f) => EinbezugDto::Flaechenrand {
            flaeche: face_to(f),
        },
        IncludeRef::Vertex(v) => EinbezugDto::Ecke {
            kante: edge_to(&v.edge),
            ende: v.end,
        },
        IncludeRef::Plane(p) => EinbezugDto::Ebene {
            ebene: match p {
                PlaneId::Standard(s) => base_to(&PlaneBase::Standard(*s)),
                PlaneId::Feature(id) => base_to(&PlaneBase::Plane(*id)),
            },
        },
        IncludeRef::Axis(a) => EinbezugDto::Achse {
            achse: axis_name(*a).into(),
        },
        IncludeRef::ConstructionPoint(id) => EinbezugDto::Konstruktionspunkt { feature: id.0 },
        IncludeRef::Origin => EinbezugDto::Ursprung,
        IncludeRef::SketchCurve { sketch, curve } => EinbezugDto::Skizzenkurve {
            skizze: sketch.0,
            kurve: curve.index(),
        },
    }
}

fn include_from(origin: &str, name: &str, d: &EinbezugDto) -> Result<IncludeRef> {
    Ok(match d {
        EinbezugDto::Kante { kante } => IncludeRef::Edge(edge_from(origin, name, kante)?),
        EinbezugDto::Flaechenrand { flaeche } => {
            IncludeRef::FaceBoundary(face_from(origin, name, flaeche)?)
        }
        EinbezugDto::Ecke { kante, ende } => IncludeRef::Vertex(VertexRef {
            edge: edge_from(origin, name, kante)?,
            end: *ende,
        }),
        EinbezugDto::Ebene { ebene } => IncludeRef::Plane(match base_from(origin, name, ebene)? {
            PlaneBase::Standard(s) => PlaneId::Standard(s),
            PlaneBase::Plane(id) => PlaneId::Feature(id),
            // Eine Fläche ist keine Ebene zum Einbeziehen (dafür gibt es den Flächenrand).
            PlaneBase::Face(_) => {
                return Err(feature_corrupt(origin, name, tr!("proj-copy-source-count")));
            }
        }),
        EinbezugDto::Achse { achse } => IncludeRef::Axis(match achse.as_str() {
            "x" => StdAxis::X,
            "y" => StdAxis::Y,
            "z" => StdAxis::Z,
            other => {
                return Err(feature_corrupt(
                    origin,
                    name,
                    tr!("proj-unknown-axis", value = other),
                ));
            }
        }),
        EinbezugDto::Konstruktionspunkt { feature } => {
            IncludeRef::ConstructionPoint(FeatureId(*feature))
        }
        EinbezugDto::Ursprung => IncludeRef::Origin,
        EinbezugDto::Skizzenkurve { skizze, kurve } => IncludeRef::SketchCurve {
            sketch: FeatureId(*skizze),
            curve: CurveId::from_index(sketch_index(origin, name, *kurve)?),
        },
    })
}

fn curve_source_to(s: &CurveSource) -> KurvenquelleDto {
    match s {
        CurveSource::SketchCurve { sketch, curve } => KurvenquelleDto::Skizzenkurve {
            skizze: sketch.0,
            kurve: curve.index(),
        },
        CurveSource::Edge(e) => KurvenquelleDto::Kante { kante: edge_to(e) },
        CurveSource::Vertex(v) => KurvenquelleDto::Ecke {
            kante: edge_to(&v.edge),
            ende: v.end,
        },
        CurveSource::ConstructionPoint(id) => KurvenquelleDto::Konstruktionspunkt { feature: id.0 },
    }
}

fn curve_source_from(origin: &str, name: &str, s: &KurvenquelleDto) -> Result<CurveSource> {
    Ok(match s {
        KurvenquelleDto::Skizzenkurve { skizze, kurve } => CurveSource::SketchCurve {
            sketch: FeatureId(*skizze),
            curve: CurveId::from_index(sketch_index(origin, name, *kurve)?),
        },
        KurvenquelleDto::Kante { kante } => CurveSource::Edge(edge_from(origin, name, kante)?),
        KurvenquelleDto::Ecke { kante, ende } => CurveSource::Vertex(VertexRef {
            edge: edge_from(origin, name, kante)?,
            end: *ende,
        }),
        KurvenquelleDto::Konstruktionspunkt { feature } => {
            CurveSource::ConstructionPoint(FeatureId(*feature))
        }
    })
}

/// Liest eine Ebene als Ziel; eine Körperfläche steht als `flaeche`, nicht als Ebene.
fn plane_id_from(origin: &str, name: &str, ebene: &PlaneDto) -> Result<PlaneId> {
    match base_from(origin, name, ebene)? {
        PlaneBase::Standard(s) => Ok(PlaneId::Standard(s)),
        PlaneBase::Plane(id) => Ok(PlaneId::Feature(id)),
        PlaneBase::Face(_) => Err(feature_corrupt(origin, name, tr!("proj-copy-source-count"))),
    }
}

fn plane_id_to(p: &PlaneId) -> PlaneDto {
    match p {
        PlaneId::Standard(s) => base_to(&PlaneBase::Standard(*s)),
        PlaneId::Feature(id) => base_to(&PlaneBase::Plane(*id)),
    }
}

fn intersection_to(p: &IntersectionSpec) -> SchnittkurveDto {
    SchnittkurveDto {
        kurven: p.curves.iter().map(curve_source_to).collect(),
        ziele: p
            .targets
            .iter()
            .map(|t| match t {
                IntersectTarget::Face(r) => SchnittzielDto::Flaeche {
                    flaeche: face_to(r),
                },
                IntersectTarget::Plane(p) => SchnittzielDto::Ebene {
                    ebene: plane_id_to(p),
                },
                IntersectTarget::SketchCurve { sketch, curve } => SchnittzielDto::Skizzenkurve {
                    skizze: sketch.0,
                    kurve: curve.index(),
                },
            })
            .collect(),
    }
}

/// Liest die „Schnittkurve“ mit der Längengrenze der Listen.
fn intersection_from(origin: &str, name: &str, d: &SchnittkurveDto) -> Result<IntersectionSpec> {
    check_len(origin, &|| tr!("proj-l-edge-copies"), d.kurven.len())?;
    check_len(origin, &|| tr!("proj-l-edge-copies"), d.ziele.len())?;
    let curves = d
        .kurven
        .iter()
        .map(|s| curve_source_from(origin, name, s))
        .collect::<Result<Vec<_>>>()?;
    let targets = d
        .ziele
        .iter()
        .map(|t| {
            Ok(match t {
                SchnittzielDto::Flaeche { flaeche } => {
                    IntersectTarget::Face(face_from(origin, name, flaeche)?)
                }
                SchnittzielDto::Ebene { ebene } => {
                    IntersectTarget::Plane(plane_id_from(origin, name, ebene)?)
                }
                SchnittzielDto::Skizzenkurve { skizze, kurve } => IntersectTarget::SketchCurve {
                    sketch: FeatureId(*skizze),
                    curve: CurveId::from_index(sketch_index(origin, name, *kurve)?),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(IntersectionSpec { curves, targets })
}

fn on_surface_to(p: &SurfaceProjection) -> AufFlaecheDto {
    AufFlaecheDto {
        quellen: p.sources.iter().map(curve_source_to).collect(),
        flaechen: p
            .faces
            .iter()
            .map(|f| match f {
                FaceTarget::Face(r) => FlaechenzielDto::Flaeche {
                    flaeche: face_to(r),
                },
                FaceTarget::Plane(p) => FlaechenzielDto::Ebene {
                    ebene: plane_id_to(p),
                },
            })
            .collect(),
        art: match p.kind {
            ProjectType::ClosestPoint => ProjektionsartDto::NaechsterPunkt,
            ProjectType::AlongVector(d) => ProjektionsartDto::EntlangVektor { richtung: v3(d) },
        },
    }
}

/// Liest „Auf Fläche projizieren“ mit der Längengrenze der Listen.
fn on_surface_from(origin: &str, name: &str, d: &AufFlaecheDto) -> Result<SurfaceProjection> {
    check_len(origin, &|| tr!("proj-l-edge-copies"), d.quellen.len())?;
    check_len(origin, &|| tr!("proj-l-edge-copies"), d.flaechen.len())?;
    let sources = d
        .quellen
        .iter()
        .map(|s| curve_source_from(origin, name, s))
        .collect::<Result<Vec<_>>>()?;
    let faces = d
        .flaechen
        .iter()
        .map(|f| {
            Ok(match f {
                FlaechenzielDto::Flaeche { flaeche } => {
                    FaceTarget::Face(face_from(origin, name, flaeche)?)
                }
                FlaechenzielDto::Ebene { ebene } => {
                    FaceTarget::Plane(plane_id_from(origin, name, ebene)?)
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let kind = match &d.art {
        ProjektionsartDto::NaechsterPunkt => ProjectType::ClosestPoint,
        ProjektionsartDto::EntlangVektor { richtung } => ProjectType::AlongVector(dv3(
            origin,
            &|| field_label(name, tr!("proj-l-direction")),
            *richtung,
        )?),
    };
    Ok(SurfaceProjection {
        sources,
        faces,
        kind,
    })
}

/// Liest eine Projektion; genau ein Ziel (`kurve` oder `punkt`) und genau eine Herkunft müssen
/// stehen.
fn copy_from(origin: &str, name: &str, d: &EdgeCopyDto) -> Result<EdgeCopy> {
    let corrupt = || feature_corrupt(origin, name, tr!("proj-copy-source-count"));
    let sources = [
        d.kante.is_some(),
        d.schnitt.is_some(),
        d.silhouette.is_some(),
        d.einbezug.is_some(),
        d.skizzenkurve.is_some(),
        d.auf_flaeche.is_some(),
        d.schnittkurve.is_some(),
        d.drehprofil.is_some(),
        d.isokurve.is_some(),
    ];
    if sources.iter().filter(|s| **s).count() != 1 {
        return Err(corrupt());
    }
    let source = if let Some(e) = &d.kante {
        CopySource::Edge(edge_from(origin, name, e)?)
    } else if let Some(f) = &d.schnitt {
        CopySource::Section {
            face: face_from(origin, name, &f.flaeche)?,
            index: f.index,
        }
    } else if let Some(f) = &d.silhouette {
        CopySource::Silhouette {
            face: face_from(origin, name, &f.flaeche)?,
            index: f.index,
        }
    } else if let Some(r) = &d.einbezug {
        CopySource::Include(include_from(origin, name, r)?)
    } else if let Some(k) = &d.skizzenkurve {
        CopySource::SketchCurve {
            sketch: FeatureId(k.skizze),
            curve: CurveId::from_index(sketch_index(origin, name, k.kurve)?),
        }
    } else if let Some(p) = &d.auf_flaeche {
        CopySource::OnSurface(on_surface_from(origin, name, p)?)
    } else if let Some(p) = &d.schnittkurve {
        CopySource::Intersection(intersection_from(origin, name, p)?)
    } else if let Some(p) = &d.drehprofil {
        CopySource::SpunProfile(spun_from(origin, name, p)?)
    } else if let Some(p) = &d.isokurve {
        iso_from(origin, name, p)?
    } else {
        return Err(corrupt());
    };
    let target = match (d.kurve, d.punkt) {
        (Some(k), None) => CopyTarget::Curve(CurveId::from_index(k)),
        (None, Some(p)) => CopyTarget::Point(PointId::from_index(p)),
        _ => return Err(corrupt()),
    };
    Ok(EdgeCopy {
        target,
        source,
        piece: d.stueck,
        // 0 ist keine Stückzahl; sie wird wie eine fehlende (1) gelesen.
        of: d.stuecke.max(1),
    })
}

/// Nummer einer Kurve oder eines Punkts in einer anderen Skizze. Größer als jede erlaubte Liste
/// ist sie beschädigt (`from_index` schnitte sie sonst ab und träfe womöglich eine andere Kurve).
fn sketch_index(origin: &str, name: &str, i: usize) -> Result<usize> {
    if i >= MAX_LIST {
        return Err(feature_corrupt(
            origin,
            name,
            tr!("proj-plane-ref-index", value = i.to_string()),
        ));
    }
    Ok(i)
}

fn plane_to(p: &PlaneFeature) -> PlaneFeatureDto {
    match &p.def {
        PlaneDef::Offset { base, distance } => PlaneFeatureDto::Versatz {
            basis: base_to(base),
            abstand: *distance,
        },
        PlaneDef::Angle { axis, angle } => PlaneFeatureDto::Winkel {
            achse: axis_ref_to(axis),
            winkel: *angle,
        },
        PlaneDef::Mid { a, b } => PlaneFeatureDto::Mitte {
            a: base_to(a),
            b: base_to(b),
        },
        PlaneDef::ThreePoints(points) => PlaneFeatureDto::DreiPunkte {
            punkte: points.each_ref().map(|p| match p {
                PointRef::Origin => PunktDto::Ursprung,
                PointRef::EdgeEnd { edge, end } => PunktDto::Kantenende {
                    kante: edge_to(edge),
                    ende: *end,
                },
                PointRef::SketchPoint { sketch, point } => PunktDto::Skizzenpunkt {
                    skizze: sketch.0,
                    punkt: point.index(),
                },
            }),
        },
        PlaneDef::Tangent { face, base, angle } => PlaneFeatureDto::Tangential {
            flaeche: face_to(face),
            basis: base_to(base),
            winkel: *angle,
        },
    }
}

fn plane_from(origin: &str, name: &str, d: &PlaneFeatureDto) -> Result<PlaneFeature> {
    let n = |label: Label, x: f64| num(origin, &|| field_label(name, label()), x);
    let point = |p: &PunktDto| -> Result<PointRef> {
        Ok(match p {
            PunktDto::Ursprung => PointRef::Origin,
            PunktDto::Kantenende { kante, ende } => PointRef::EdgeEnd {
                edge: edge_from(origin, name, kante)?,
                end: *ende,
            },
            PunktDto::Skizzenpunkt { skizze, punkt } => PointRef::SketchPoint {
                sketch: FeatureId(*skizze),
                point: PointId::from_index(sketch_index(origin, name, *punkt)?),
            },
        })
    };
    let def = match d {
        PlaneFeatureDto::Versatz { basis, abstand } => PlaneDef::Offset {
            base: base_from(origin, name, basis)?,
            distance: n(|| tr!("proj-l-distance"), *abstand)?,
        },
        PlaneFeatureDto::Winkel { achse, winkel } => PlaneDef::Angle {
            axis: match achse {
                AchseDto::Standard { achse } => AxisRef::Standard(match achse.as_str() {
                    "x" => StdAxis::X,
                    "y" => StdAxis::Y,
                    "z" => StdAxis::Z,
                    other => {
                        return Err(feature_corrupt(
                            origin,
                            name,
                            tr!("proj-unknown-axis", value = other),
                        ));
                    }
                }),
                AchseDto::Kante { kante } => AxisRef::Edge(edge_from(origin, name, kante)?),
                AchseDto::Skizzenlinie { skizze, kurve } => AxisRef::SketchLine {
                    sketch: FeatureId(*skizze),
                    curve: CurveId::from_index(sketch_index(origin, name, *kurve)?),
                },
            },
            angle: n(|| tr!("proj-l-angle"), *winkel)?,
        },
        PlaneFeatureDto::Mitte { a, b } => PlaneDef::Mid {
            a: base_from(origin, name, a)?,
            b: base_from(origin, name, b)?,
        },
        PlaneFeatureDto::DreiPunkte { punkte: [a, b, c] } => {
            PlaneDef::ThreePoints([point(a)?, point(b)?, point(c)?])
        }
        PlaneFeatureDto::Tangential {
            flaeche,
            basis,
            winkel,
        } => PlaneDef::Tangent {
            face: face_from(origin, name, flaeche)?,
            base: base_from(origin, name, basis)?,
            angle: n(|| tr!("proj-l-angle"), *winkel)?,
        },
    };
    Ok(PlaneFeature { def })
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
    let n = |label: Label, x: f64| num(origin, &|| field_label(name, label()), x);
    Ok(match d {
        BlendArtDto::Rundung { radius } => BlendArt::Rundung {
            radius: n(|| tr!("proj-l-radius"), *radius)?,
        },
        BlendArtDto::RundungVariabel { punkte } => {
            check_len(origin, &|| tr!("proj-l-radius-points"), punkte.len())?;
            BlendArt::RundungVariabel {
                punkte: punkte
                    .iter()
                    .map(|[p, r]| {
                        Ok((
                            n(|| tr!("proj-l-position"), *p)?,
                            n(|| tr!("proj-l-radius"), *r)?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?,
            }
        }
        BlendArtDto::FaseGleich { abstand } => BlendArt::FaseGleich {
            abstand: n(|| tr!("proj-l-distance"), *abstand)?,
        },
        BlendArtDto::FaseZwei { abstand1, abstand2 } => BlendArt::FaseZwei {
            abstand1: n(|| tr!("proj-l-distance-1"), *abstand1)?,
            abstand2: n(|| tr!("proj-l-distance-2"), *abstand2)?,
        },
        BlendArtDto::FaseWinkel { abstand, winkel } => BlendArt::FaseWinkel {
            abstand: n(|| tr!("proj-l-distance"), *abstand)?,
            winkel: n(|| tr!("proj-l-angle"), *winkel)?,
        },
    })
}

/// Liest die Art eines Features; `dissolved` zählt die beim Laden aufgelösten Muster mit.
fn kind_from(origin: &str, name: &str, d: &KindDto, dissolved: &mut usize) -> Result<FeatureKind> {
    let n = |label: Label, x: f64| num(origin, &|| field_label(name, label()), x);
    let v = |label: Label, x: V3| dv3(origin, &|| field_label(name, label()), x);
    Ok(match d {
        KindDto::Skizze(s) => {
            check_len(origin, &|| tr!("proj-l-edge-copies"), s.kopien.len())?;
            let (sketch, n) = sketch_from(origin, name, &s.skizze)?;
            *dissolved += n;
            let formulas = sketch_formulas_from(origin, name, s, &sketch)?;
            let text_formulas = text_formulas_from(origin, name, s, &sketch)?;
            let mut sf = SketchFeature {
                sketch,
                plane: match base_from(origin, name, &s.lage)? {
                    PlaneBase::Standard(p) => SketchPlane::Standard(p),
                    PlaneBase::Face(r) => SketchPlane::OnFace(r),
                    PlaneBase::Plane(id) => SketchPlane::OnPlane(id),
                },
                copies: s
                    .kopien
                    .iter()
                    .map(|c| copy_from(origin, name, c))
                    .collect::<Result<Vec<_>>>()?,
                formulas,
                text_formulas,
            };
            // Eine Projektion, deren Kurve in der Skizze nicht mehr lebt, ist tot und fällt beim
            // Laden weg (wie beim Bearbeiten).
            sf.prune_copies();
            FeatureKind::Sketch(sf)
        }
        KindDto::Quader { groesse, ursprung } => FeatureKind::Box {
            size: v(|| tr!("proj-l-size"), *groesse)?,
            origin: v(|| tr!("proj-l-origin"), *ursprung)?,
        },
        KindDto::Zylinder {
            radius,
            hoehe,
            ursprung,
        } => FeatureKind::Cylinder {
            radius: n(|| tr!("proj-l-radius"), *radius)?,
            height: n(|| tr!("proj-l-height"), *hoehe)?,
            origin: v(|| tr!("proj-l-origin"), *ursprung)?,
        },
        KindDto::Kugel { radius, ursprung } => FeatureKind::Sphere {
            radius: n(|| tr!("proj-l-radius"), *radius)?,
            origin: v(|| tr!("proj-l-origin"), *ursprung)?,
        },
        KindDto::Kegel {
            radius,
            hoehe,
            ursprung,
        } => FeatureKind::Cone {
            radius: n(|| tr!("proj-l-radius"), *radius)?,
            height: n(|| tr!("proj-l-height"), *hoehe)?,
            origin: v(|| tr!("proj-l-origin"), *ursprung)?,
        },
        KindDto::Extrusion {
            skizze,
            profil,
            profile,
            hoehe,
        } => FeatureKind::Extrude {
            sketch: FeatureId(*skizze),
            profiles: profile_sel_from(origin, name, *profil, profile.as_deref())?,
            height: n(|| tr!("proj-l-height"), *hoehe)?,
        },
        KindDto::Rotation {
            skizze,
            profil,
            profile,
            achse,
        } => FeatureKind::Revolve {
            sketch: FeatureId(*skizze),
            profiles: profile_sel_from(origin, name, *profil, profile.as_deref())?,
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
                        tr!(
                            "proj-ctx-feature",
                            name = name,
                            msg = tr!("proj-unknown-operation", value = other)
                        ),
                    ));
                }
            },
        },
        KindDto::Verschieben { koerper, versatz } => FeatureKind::Move {
            body: body_from(koerper),
            delta: v(|| tr!("proj-l-offset"), *versatz)?,
        },
        KindDto::Verrunden {
            koerper,
            kanten,
            art,
            tangentenkette,
        } => {
            check_len(origin, &|| tr!("proj-l-edges"), kanten.len())?;
            if kanten.is_empty() {
                return Err(ProjectError::corrupt(
                    origin,
                    tr!(
                        "proj-ctx-feature",
                        name = name,
                        msg = tr!("proj-no-edge-selected")
                    ),
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
                offset: v(|| tr!("proj-l-position"), *versatz)?,
                rotation: DQuat::from_xyzw(
                    n(|| tr!("proj-l-rotation"), x)?,
                    n(|| tr!("proj-l-rotation"), y)?,
                    n(|| tr!("proj-l-rotation"), z)?,
                    n(|| tr!("proj-l-rotation"), w)?,
                ),
                scale: n(|| tr!("proj-l-scale"), *massstab)?,
            }
        }
        KindDto::StepKoerper { form } => FeatureKind::StepBody {
            shape: StepShapeId(*form),
        },
        KindDto::Konstruktionsebene(p) => FeatureKind::Plane(plane_from(origin, name, p)?),
        KindDto::Leinwand(c) => FeatureKind::Canvas(canvas_from(origin, name, c)?),
    })
}

// ---- Parameter und Formeln (Format 9) ---------------------------------------------------

const PARAM_UNITS: [(ParamUnit, &str); 3] = [
    (ParamUnit::Length, "laenge"),
    (ParamUnit::Angle, "winkel"),
    (ParamUnit::Number, "zahl"),
];

fn too_long(text: &str, max: usize) -> bool {
    text.chars().count() > max
}

/// Prüft die Länge einer Formel aus der Datei; ob sie sich auswerten lässt, entscheidet erst
/// das Dokument (eine kaputte Feature-Formel lädt, Spec §4a.7).
fn formula_from(origin: &str, what: &str, text: &str) -> Result<String> {
    if too_long(text, MAX_FORMULA_CHARS) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-formula-too-long",
                what = what,
                max = MAX_FORMULA_CHARS.to_string()
            ),
        ));
    }
    Ok(text.to_string())
}

/// Formeln einer Skizze: nur an lebenden, bestimmenden Maßen (Index in `abhaengigkeiten`),
/// jede Abhängigkeit höchstens einmal. An abgeschalteten Maßen bleiben Formeln erlaubt.
fn sketch_formulas_from(
    origin: &str,
    feature: &str,
    d: &SketchFeatureDto,
    sketch: &Sketch,
) -> Result<SketchFormulas> {
    let bad =
        |m: &str| ProjectError::corrupt(origin, tr!("proj-ctx-feature", name = feature, msg = m));
    let mut out = SketchFormulas::new();
    for f in &d.formeln {
        // Erst gegen die Länge der Liste prüfen: `from_index` schneidet sonst große Zahlen ab.
        if f.abhaengigkeit >= d.skizze.abhaengigkeiten.len() {
            return Err(bad(&tr!("proj-formula-missing-constraint")));
        }
        let id = ConstraintId::from_index(f.abhaengigkeit);
        match sketch.constraint(id) {
            None => return Err(bad(&tr!("proj-formula-missing-constraint"))),
            Some(c) if !c.kind.is_dimension() || c.driven => {
                return Err(bad(&tr!("proj-formula-not-driving")));
            }
            Some(_) => {}
        }
        let text = formula_from(origin, &tr!("proj-w-quoted", name = feature), &f.formel)?;
        if out.insert(id, text).is_some() {
            return Err(bad(&tr!("proj-formula-twice")));
        }
    }
    Ok(out)
}

/// Höhenformeln der Texte einer Skizze (ab Format 22): nur an lebenden Texten (Index in
/// `texte`), jeder Text höchstens einmal.
fn text_formulas_from(
    origin: &str,
    feature: &str,
    d: &SketchFeatureDto,
    sketch: &Sketch,
) -> Result<TextFormulas> {
    let bad =
        |m: &str| ProjectError::corrupt(origin, tr!("proj-ctx-feature", name = feature, msg = m));
    let mut out = TextFormulas::new();
    for f in &d.hoehenformeln {
        // Erst gegen die Länge der Liste prüfen: `from_index` schneidet sonst große Zahlen ab.
        if f.text >= d.skizze.texte.len() {
            return Err(bad(&tr!("proj-text-formula-missing-text")));
        }
        let id = TextId::from_index(f.text);
        if sketch.text(id).is_none() {
            return Err(bad(&tr!("proj-text-formula-missing-text")));
        }
        let text = formula_from(origin, &tr!("proj-w-quoted", name = feature), &f.formel)?;
        if out.insert(id, text).is_some() {
            return Err(bad(&tr!("proj-text-formula-twice")));
        }
    }
    Ok(out)
}

/// Liest die Skizzengruppen (ab Format 22): Kennungen eindeutig, die übergeordnete Gruppe steht
/// in der Datei (ohne Kreis), jede genannte Skizze ist ein Skizzen-Feature der Datei und steht
/// in höchstens einer Gruppe. Die nächste Kennung folgt auf die größte (sie steht nicht in der
/// Datei).
fn sketch_groups_from(origin: &str, file: &VersionFile) -> Result<SketchGroups> {
    let bad = |m: String| ProjectError::corrupt(origin, m);
    let sketches: HashSet<u64> = file
        .features
        .iter()
        .filter(|f| matches!(f.art, KindDto::Skizze(_)))
        .map(|f| f.id)
        .collect();
    let mut out = SketchGroups::default();
    let mut parent_of: HashMap<u64, Option<u64>> = HashMap::new();
    for g in &file.skizzengruppen {
        if parent_of.insert(g.kennung, g.eltern).is_some() {
            return Err(bad(tr!(
                "proj-sketch-group-twice",
                id = g.kennung.to_string()
            )));
        }
    }
    for g in &file.skizzengruppen {
        // Der Weg nach oben muss nach höchstens so vielen Schritten, wie es Gruppen gibt, oben
        // ankommen; sonst fehlt eine Gruppe oder es gibt einen Kreis.
        let mut at = g.eltern;
        let mut steps = 0;
        while let Some(p) = at {
            match parent_of.get(&p) {
                Some(next) if steps < parent_of.len() && p != g.kennung => {
                    at = *next;
                    steps += 1;
                }
                _ => {
                    return Err(bad(tr!(
                        "proj-sketch-group-parent",
                        id = g.kennung.to_string()
                    )));
                }
            }
        }
        check_len(origin, &|| tr!("proj-l-sketch-groups"), g.skizzen.len())?;
        for s in &g.skizzen {
            if !sketches.contains(s) {
                return Err(bad(tr!(
                    "proj-sketch-group-not-sketch",
                    id = g.kennung.to_string(),
                    feature = s.to_string()
                )));
            }
            if out
                .member
                .insert(FeatureId(*s), SketchGroupId(g.kennung))
                .is_some()
            {
                return Err(bad(tr!(
                    "proj-sketch-group-member-twice",
                    feature = s.to_string()
                )));
            }
        }
        out.groups.push(SketchGroup {
            id: SketchGroupId(g.kennung),
            name: g.name.clone(),
            parent: g.eltern.map(SketchGroupId),
            hidden: g.aus,
        });
    }
    out.next = match out.groups.iter().map(|g| g.id.0).max() {
        Some(m) => m
            .checked_add(1)
            .ok_or_else(|| bad(tr!("proj-sketch-group-twice", id = m.to_string())))?,
        None => 0,
    };
    Ok(out)
}

/// Skizzengruppen für die Datei, in Browser-Reihenfolge; die Skizzen je Gruppe aufsteigend.
fn sketch_groups_to(g: &SketchGroups) -> Vec<SkizzengruppeDto> {
    g.groups
        .iter()
        .map(|grp| SkizzengruppeDto {
            kennung: grp.id.0,
            name: grp.name.clone(),
            eltern: grp.parent.map(|p| p.0),
            aus: grp.hidden,
            skizzen: g
                .member
                .iter()
                .filter(|(_, m)| **m == grp.id)
                .map(|(f, _)| f.0)
                .collect(),
        })
        .collect()
}

/// Formeln der Zahlenfelder eines Features: bekannte Schlüssel, die die Art auch hat.
fn feature_formulas_from(origin: &str, f: &FeatureDto, kind: &FeatureKind) -> Result<Formulas> {
    let bad = |m: String| {
        ProjectError::corrupt(
            origin,
            tr!("proj-ctx-feature", name = f.name.as_str(), msg = m),
        )
    };
    let keys = field_keys(kind);
    let mut out = Formulas::new();
    for (key, text) in &f.formeln {
        let k = FieldKey::from_file_key(key)
            .ok_or_else(|| bad(tr!("proj-formula-unknown-field", key = quoted(key))))?;
        if !keys.contains(&k) {
            return Err(bad(tr!("proj-formula-field-none", key = quoted(key))));
        }
        out.insert(
            k,
            formula_from(origin, &tr!("proj-w-quoted", name = f.name.as_str()), text)?,
        );
    }
    Ok(out)
}

/// Parametertabelle samt Zähler. Hier geprüft: Anzahl, Längen, Art, Kennungen; die Tabelle
/// selbst (Namensregeln, doppelte Namen, Kreise, unbekannte Namen) prüft
/// `Document::from_snapshot`.
fn parameters_from(origin: &str, file: &VersionFile) -> Result<(Vec<Parameter>, u64)> {
    if file.parameter.len() > MAX_PARAMETERS {
        return Err(ProjectError::corrupt(
            origin,
            tr!("proj-too-many-parameters", max = MAX_PARAMETERS.to_string()),
        ));
    }
    let next = counter(
        origin,
        &tr!("proj-w-parameter-counter"),
        file.naechster_parameter,
        file.parameter.iter().map(|p| p.id),
    )?;
    let mut ids = HashSet::with_capacity(file.parameter.len());
    let mut out = Vec::with_capacity(file.parameter.len());
    for p in &file.parameter {
        if too_long(&p.name, MAX_PARAM_NAME_CHARS) {
            return Err(ProjectError::corrupt(
                origin,
                tr!(
                    "proj-param-name-too-long",
                    max = MAX_PARAM_NAME_CHARS.to_string()
                ),
            ));
        }
        let bad = |m: String| {
            ProjectError::corrupt(
                origin,
                tr!("proj-ctx-parameter", name = p.name.as_str(), msg = m),
            )
        };
        if !ids.insert(p.id) {
            return Err(bad(tr!("proj-duplicate-id")));
        }
        let unit = value_of(&PARAM_UNITS, &p.art)
            .ok_or_else(|| bad(tr!("proj-param-unknown-unit", value = quoted(&p.art))))?;
        if too_long(&p.kommentar, MAX_COMMENT_CHARS) {
            return Err(bad(tr!(
                "proj-param-comment-too-long",
                max = MAX_COMMENT_CHARS.to_string()
            )));
        }
        out.push(Parameter {
            id: ParamId(p.id),
            name: p.name.clone(),
            formula: formula_from(
                origin,
                &tr!("proj-w-parameter", name = p.name.as_str()),
                &p.formel,
            )?,
            unit,
            comment: p.kommentar.clone(),
        });
    }
    Ok((out, next))
}

fn parameter_to(p: &Parameter) -> ParameterDto {
    ParameterDto {
        id: p.id.0,
        name: p.name.clone(),
        formel: p.formula.clone(),
        art: name_of(&PARAM_UNITS, p.unit),
        kommentar: p.comment.clone(),
    }
}

/// Hat die Datei Parameter oder Formeln (erst ab Format 9)?
fn has_format_9_data(file: &VersionFile) -> bool {
    !file.parameter.is_empty()
        || file.naechster_parameter != 0
        || file.features.iter().any(|f| {
            !f.formeln.is_empty() || matches!(&f.art, KindDto::Skizze(s) if !s.formeln.is_empty())
        })
}

// ---- Netze ------------------------------------------------------------------------------

/// Liest die Netze einer Datei. Vor dem Dekodieren werden Länge der Zeichenkette (auch in
/// Summe) und der Kopf gegen die Grenzen und gegen die Gesamtlänge geprüft (Spec §7).
fn meshes_from(origin: &str, list: &[MeshDto]) -> Result<BTreeMap<MeshId, Arc<ExchangeMesh>>> {
    let mut total: u64 = 0;
    let mut out = BTreeMap::new();
    for d in list {
        let bad = |m: &str| {
            ProjectError::corrupt(origin, tr!("proj-ctx-mesh", id = d.id.to_string(), msg = m))
        };
        if d.id == 0 {
            return Err(bad(&tr!("proj-bad-id")));
        }
        let len = base64::decoded_len(&d.daten).map_err(|e| bad(&e.to_string()))? as u64;
        total = total.saturating_add(len);
        if len > MAX_MESH_BYTES || total > MAX_MESH_BYTES {
            return Err(bad(&tr!("proj-mesh-too-big")));
        }
        let head = base64::decode_prefix(&d.daten, 8).map_err(|e| bad(&e.to_string()))?;
        let expected =
            ExchangeMesh::byte_len_from_header(&head).map_err(|e| bad(&e.to_string()))?;
        if expected != len {
            return Err(bad(&tr!("proj-mesh-length-mismatch")));
        }
        let bytes = base64::decode(&d.daten).map_err(|e| bad(&e.to_string()))?;
        let mesh = ExchangeMesh::from_bytes(&bytes).map_err(|e| bad(&e.to_string()))?;
        if out.insert(MeshId(d.id), Arc::new(mesh)).is_some() {
            return Err(bad(&tr!("proj-duplicate-id")));
        }
    }
    Ok(out)
}

// ---- STEP-Formen (Format 10) ------------------------------------------------------------

/// Formen für die Datei: nur die von STEP-Körpern benutzten, aufsteigend nach Kennung. Zeigen
/// mehrere Kennungen auf dieselbe Form (dasselbe `Arc`, etwa zwei Körper aus einem Eintrag),
/// steht der Text nur einmal in der Datei, unter der kleinsten dieser Kennungen. Liefert die
/// Einträge und die Zuordnung Formkennung → Kennung in der Datei. Baut keinen Körper.
fn step_forms_to(s: &Snapshot) -> (Vec<StepFormDto>, BTreeMap<StepShapeId, u64>) {
    let used: HashSet<StepShapeId> = s
        .features
        .iter()
        .filter_map(|f| match &f.kind {
            FeatureKind::StepBody { shape } => Some(*shape),
            _ => None,
        })
        .collect();
    let mut written: Vec<(&Arc<StepShape>, u64)> = Vec::new();
    let mut forms = Vec::new();
    let mut map = BTreeMap::new();
    for (id, shape) in s.step_shapes.iter().filter(|(id, _)| used.contains(id)) {
        if let Some((_, file_id)) = written.iter().find(|(w, _)| Arc::ptr_eq(w, shape)) {
            map.insert(*id, *file_id);
            continue;
        }
        written.push((shape, id.0));
        map.insert(*id, id.0);
        forms.push(StepFormDto {
            id: id.0,
            genauigkeit: shape.tolerance_mm(),
            genauigkeit_unbekannt: !shape.tolerance_known(),
            text: shape.text().to_string(),
        });
    }
    (forms, map)
}

/// Liest die STEP-Formen einer Datei. Geprüft werden Kennung (nicht 0, nicht doppelt), die
/// Summe der Textlängen (`planora_feature::MAX_STEP_TEXT_SUM_BYTES`, dieselbe Grenze wie beim
/// Import) und je Form nur Kopf und Genauigkeit (`StepShape::new`, Plan P14);
/// gebaut wird erst beim Rechnen, ein kaputter Körpertext lädt mit Fehlerstatus am Feature.
fn step_forms_from(
    origin: &str,
    list: Vec<StepFormDto>,
) -> Result<BTreeMap<StepShapeId, Arc<StepShape>>> {
    check_len(origin, &|| tr!("proj-l-step-shapes"), list.len())?;
    let total = list
        .iter()
        .fold(0usize, |sum, f| sum.saturating_add(f.text.len()));
    if total > step_text_sum_limit() {
        return Err(ProjectError::corrupt(
            origin,
            tr!("proj-step-forms-too-big"),
        ));
    }
    let mut out = BTreeMap::new();
    for f in list {
        let bad = |m: &str| {
            ProjectError::corrupt(
                origin,
                tr!("proj-ctx-step-form", id = f.id.to_string(), msg = m),
            )
        };
        if f.id == 0 {
            return Err(bad(&tr!("proj-bad-id")));
        }
        if out.contains_key(&StepShapeId(f.id)) {
            return Err(bad(&tr!("proj-duplicate-id")));
        }
        let mut shape = StepShape::new(f.text, f.genauigkeit).map_err(|m| bad(&m))?;
        if f.genauigkeit_unbekannt {
            shape = shape.accuracy_unknown();
        }
        out.insert(StepShapeId(f.id), Arc::new(shape));
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
fn pos(origin: &str, what: What, x: f64) -> Result<f64> {
    if x.is_finite() && x.abs() <= MAX_POS_MM {
        Ok(x)
    } else {
        Err(ProjectError::corrupt(
            origin,
            tr!("proj-number-invalid", what = what()),
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
    let bad_id = || ProjectError::corrupt(origin, tr!("proj-counter-bad-id", what = what));
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
            tr!("proj-counter-invalid", what = what),
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
                ProjectError::Invalid(tr!("proj-dim-ref-mismatch", id = m.id.0.to_string()))
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
    dv3(
        origin,
        &|| tr!("proj-what-label", what = what, label = tr!("proj-l-hint")),
        v,
    )
}

/// Liest eine Flächenmarke eines Maßbezugs: Rolle höchstens [`MAX_ROLE_CHARS`] Zeichen,
/// Hinweispunkt wie in [`hint_from`].
fn tag_from(origin: &str, what: &str, d: &FaceRefDto) -> Result<TagData> {
    if d.rolle.chars().count() > MAX_ROLE_CHARS {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-role-too-long",
                what = what,
                max = MAX_ROLE_CHARS.to_string()
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
                ProjectError::corrupt(
                    origin,
                    tr!("proj-edge-end-unknown", what = what, value = ende.as_str()),
                )
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
                tr!("proj-tolerance-number-invalid", what = what),
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
                    tr!(
                        "proj-fit-too-long",
                        what = what,
                        max = MAX_FIT_CHARS.to_string()
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
    let what = tr!("proj-w-dimension", drawing = drawing, id = d.id.to_string());
    let bad = |m: String| {
        ProjectError::corrupt(origin, tr!("proj-ctx-what", what = what.as_str(), msg = m))
    };
    let kind = value_of(&DIM_KINDS, &d.art)
        .ok_or_else(|| bad(tr!("proj-dim-unknown-kind", value = d.art.as_str())))?;
    match views.get(&ViewId(d.ansicht)) {
        None => return Err(bad(tr!("proj-dim-no-view"))),
        Some(k) if !k.is_normal() => {
            return Err(bad(tr!("proj-dim-iso-view")));
        }
        Some(_) => {}
    }
    let (lo, hi) = ref_count(kind);
    if d.bezuege.len() < lo || d.bezuege.len() > hi {
        return Err(bad(tr!("proj-dim-wrong-refs")));
    }
    let refs = d
        .bezuege
        .iter()
        .map(|r| georef_from(origin, &what, r))
        .collect::<Result<Vec<_>>>()?;
    if d.stellen > MAX_DECIMALS {
        return Err(bad(tr!(
            "proj-dim-decimals",
            max = MAX_DECIMALS.to_string()
        )));
    }
    if d.text
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_DIM_TEXT_CHARS)
    {
        return Err(bad(tr!(
            "proj-dim-text-too-long",
            max = MAX_DIM_TEXT_CHARS.to_string()
        )));
    }
    // Eine Stelle in einer Kette ist höchstens so groß wie die Zahl der Maße einer Zeichnung
    // (gestapelte Bezugsmaße liegen sonst unbegrenzt weit draußen).
    if let Some(k) = &d.kette
        && k.reihenfolge as usize >= MAX_DIMENSIONS
    {
        return Err(bad(tr!(
            "proj-dim-chain-order",
            max = (MAX_DIMENSIONS - 1).to_string()
        )));
    }
    let chain = match &d.kette {
        None => None,
        Some(k) => Some(ChainSpec {
            group: k.gruppe,
            mode: value_of(&CHAIN_MODES, &k.modus)
                .ok_or_else(|| bad(tr!("proj-dim-unknown-chain", value = k.modus.as_str())))?,
            order: k.reihenfolge,
        }),
    };
    let p = |label: Label, x: f64| {
        pos(
            origin,
            &|| tr!("proj-what-label", what = what.as_str(), label = label()),
            x,
        )
    };
    Ok(DimSpec {
        id: DimId(d.id),
        view: ViewId(d.ansicht),
        kind,
        refs,
        offset: p(|| tr!("proj-l-distance"), d.abstand)?,
        angle: p(|| tr!("proj-l-angle"), d.winkel)?,
        text_shift: (
            p(|| tr!("proj-l-text-shift"), d.textversatz[0])?,
            p(|| tr!("proj-l-text-shift"), d.textversatz[1])?,
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
    let ctx = |m: &str| {
        ProjectError::corrupt(
            origin,
            tr!("proj-ctx-drawing", name = d.name.as_str(), msg = m),
        )
    };
    if d.masse.len() > MAX_DIMENSIONS {
        return Err(ctx(&tr!("proj-dim-too-many")));
    }
    let next = counter(
        origin,
        &tr!("proj-w-dimension-counter", name = d.name.as_str()),
        d.naechstes_mass,
        d.masse.iter().map(|m| m.id),
    )?;
    let mut ids = HashSet::with_capacity(d.masse.len());
    let mut modes: HashMap<u64, ChainMode> = HashMap::new();
    let mut out = Vec::with_capacity(d.masse.len());
    for m in &d.masse {
        if !ids.insert(m.id) {
            return Err(ctx(&tr!("proj-dim-id-twice")));
        }
        let spec = dimension_from(origin, &d.name, views, m)?;
        if let Some(c) = spec.chain
            && *modes.entry(c.group).or_insert(c.mode) != c.mode
        {
            return Err(ctx(&tr!("proj-dim-chain-mixed")));
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
        .map_err(|e| ProjectError::corrupt(origin, tr!("proj-ctx-project-info", msg = e)))?;
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
        stueckliste: i.parts_list,
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
        parts_list: b.stueckliste,
    };
    validate_sheet_info(&info).map_err(|e| {
        ProjectError::corrupt(origin, tr!("proj-ctx-drawing", name = name, msg = e))
    })?;
    Ok(info)
}

fn body_key_from(k: [u64; 2]) -> Option<BodyKey> {
    Some(BodyKey {
        feature: k[0],
        index: u32::try_from(k[1]).ok()?,
    })
}

/// Sichtbarkeit für die Datei; `None` im Grundzustand (das Feld fehlt dann).
fn visibility_to(v: &Visibility) -> Option<SichtbarkeitDto> {
    if v.is_default() {
        return None;
    }
    let planes = [
        (StandardPlane::XY, GrundebeneDto::Xy),
        (StandardPlane::XZ, GrundebeneDto::Xz),
        (StandardPlane::YZ, GrundebeneDto::Yz),
    ];
    let folders = [
        (v.folders.bodies, OrdnerDto::Koerper),
        (v.folders.sketches, OrdnerDto::Skizzen),
        (v.folders.planes, OrdnerDto::Konstruktion),
        // Nur mit einer Leinwand gesetzt (Format 19 kommt dann von der Leinwand).
        (v.folders.canvases, OrdnerDto::Leinwaende),
    ];
    Some(SichtbarkeitDto {
        koerper: v
            .bodies
            .iter()
            .map(|k| [k.feature, u64::from(k.index)])
            .collect(),
        features: v.features.iter().map(|f| f.0).collect(),
        grundebenen: planes
            .into_iter()
            .filter(|(p, _)| v.origin_shown(*p))
            .map(|(_, d)| d)
            .collect(),
        ordner: folders
            .into_iter()
            .filter(|(hidden, _)| *hidden)
            .map(|(_, d)| d)
            .collect(),
    })
}

/// Sichtbarkeit aus der Datei. Verweise auf fehlende Features fallen beim Bauen des Dokuments
/// weg ([`planora_feature::Document::set_visibility`]); ein Körperindex jenseits von `u32` nennt
/// keinen Körper und entfällt hier.
fn visibility_from(d: Option<&SichtbarkeitDto>) -> Visibility {
    let mut v = Visibility::default();
    let Some(d) = d else {
        return v;
    };
    v.bodies = d
        .koerper
        .iter()
        .filter_map(|[feature, index]| {
            Some(BodyKey {
                feature: *feature,
                index: u32::try_from(*index).ok()?,
            })
        })
        .collect();
    v.features = d.features.iter().map(|f| FeatureId(*f)).collect();
    for p in &d.grundebenen {
        v.set_origin_shown(
            match p {
                GrundebeneDto::Xy => StandardPlane::XY,
                GrundebeneDto::Xz => StandardPlane::XZ,
                GrundebeneDto::Yz => StandardPlane::YZ,
            },
            true,
        );
    }
    for f in &d.ordner {
        match f {
            OrdnerDto::Koerper => v.folders.bodies = true,
            OrdnerDto::Skizzen => v.folders.sketches = true,
            OrdnerDto::Konstruktion => v.folders.planes = true,
            OrdnerDto::Leinwaende => v.folders.canvases = true,
        }
    }
    v
}

fn overrides_to(o: &BTreeMap<BodyKey, RowOverride>) -> Vec<StuecklisteZeileDto> {
    o.iter()
        .map(|(k, v)| StuecklisteZeileDto {
            koerper: [k.feature, u64::from(k.index)],
            benennung: field_to(&v.name),
            menge: field_to(&v.qty),
            werkstoff: field_to(&v.material),
            bemerkung: field_to(&v.note),
        })
        .collect()
}

fn overrides_from(
    origin: &str,
    list: &[StuecklisteZeileDto],
) -> Result<BTreeMap<BodyKey, RowOverride>> {
    let bad = |m: String| ProjectError::corrupt(origin, tr!("proj-ctx-parts-list", msg = m));
    // Die Länge ist beim Lesen schon auf MAX_OVERRIDES begrenzt (`de_overrides`).
    let mut out = BTreeMap::new();
    for z in list {
        let key = body_key_from(z.koerper).ok_or_else(|| bad(tr!("proj-parts-bad-body")))?;
        let o = RowOverride {
            name: field_from(&z.benennung),
            qty: field_from(&z.menge),
            material: field_from(&z.werkstoff),
            note: field_from(&z.bemerkung),
        };
        validate_override(&o).map_err(bad)?;
        if o.is_auto() {
            return Err(bad(tr!("proj-parts-no-texts")));
        }
        if out.insert(key, o).is_some() {
            return Err(bad(tr!("proj-parts-body-twice")));
        }
    }
    Ok(out)
}

fn drawing_to(d: &Drawing, with_marks: bool, with_parts: bool) -> Result<ZeichnungDto> {
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
                schnitt: v.section.map(|s| SchnittDto {
                    versatz: s.offset,
                    buchstabe: s.letter.to_string(),
                    schraffur: s.hatch.on,
                    schraffurabstand: s.hatch.spacing,
                }),
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
        stuecklistenseite: match d.kind {
            DrawingKind::PartsList { page } => Some(page),
            DrawingKind::Views => None,
        },
        positionen: d
            .balloons
            .iter()
            .map(|b| PositionDto {
                id: b.id.0,
                ansicht: b.view.0,
                koerper: [b.body.feature, u64::from(b.body.index)],
                anker: [b.anchor.0, b.anchor.1],
                versatz: [b.offset.0, b.offset.1],
            })
            .collect(),
        // Der Zähler bleibt auch ohne Kreise erhalten (gelöschte Kennungen werden nicht
        // wiederverwendet), aber nur in Dateien, die ohnehin Format 7 haben.
        naechste_position: if with_parts && (!d.balloons.is_empty() || d.next_balloon > 1) {
            d.next_balloon
        } else {
            0
        },
        einheit: match d.unit {
            DisplayUnit::Millimeter => None,
            u => Some(u.id().to_string()),
        },
    })
}

fn drawing_from(origin: &str, d: &ZeichnungDto) -> Result<Drawing> {
    let ctx = |m: String| {
        ProjectError::corrupt(
            origin,
            tr!("proj-ctx-drawing", name = d.name.as_str(), msg = m),
        )
    };
    if d.ansichten.len() > MAX_VIEWS {
        return Err(ctx(tr!("proj-views-too-many")));
    }
    let paper = value_of(&PAPERS, &d.blatt.papier)
        .ok_or_else(|| ctx(tr!("proj-unknown-paper", value = d.blatt.papier.as_str())))?;
    let projection = value_of(&PROJECTIONS, &d.projektion).ok_or_else(|| {
        ctx(tr!(
            "proj-unknown-projection",
            value = d.projektion.as_str()
        ))
    })?;
    let what = tr!("proj-w-view-counter", name = d.name.as_str());
    let next_view = counter(
        origin,
        &what,
        d.naechste_ansicht,
        d.ansichten.iter().map(|v| v.id),
    )?;
    let mut views = Vec::with_capacity(d.ansichten.len());
    for v in &d.ansichten {
        let kind = value_of(&VIEW_KINDS, &v.art)
            .ok_or_else(|| ctx(tr!("proj-unknown-view-kind", value = v.art.as_str())))?;
        let [num, den] = v.massstab;
        if !(1..=1000).contains(&num) || !(1..=1000).contains(&den) {
            return Err(ctx(tr!(
                "proj-scale-range",
                num = num.to_string(),
                den = den.to_string()
            )));
        }
        let p = |label: Label, x: f64| pos(origin, &|| drawing_label(d.name.as_str(), label()), x);
        let placement = match v.lage {
            LageDto::Frei { x, y } => Placement::Free {
                x: p(|| tr!("proj-l-placement"), x)?,
                y: p(|| tr!("proj-l-placement"), y)?,
            },
            LageDto::Ausgerichtet { abstand } => Placement::Aligned {
                distance: p(|| tr!("proj-l-distance"), abstand)?,
            },
        };
        let section = v
            .schnitt
            .as_ref()
            .map(|s| -> Result<SectionSpec> {
                let what = drawing_label(d.name.as_str(), tr!("proj-l-section"));
                let mut chars = s.buchstabe.chars();
                let (Some(letter), None) = (chars.next(), chars.next()) else {
                    return Err(ctx(tr!("proj-section-letter")));
                };
                Ok(SectionSpec {
                    offset: pos(origin, &|| what.clone(), s.versatz)?,
                    letter,
                    hatch: HatchSpec {
                        on: s.schraffur,
                        spacing: pos(origin, &|| what.clone(), s.schraffurabstand)?,
                    },
                })
            })
            .transpose()?;
        views.push(ViewSpec {
            id: ViewId(v.id),
            kind,
            scale: Scale { num, den },
            placement,
            show_hidden: v.verdeckt_zeigen,
            parent: v.eltern.map(ViewId),
            centerlines: v.mittellinien.unwrap_or(false),
            section,
        });
    }
    let kinds: HashMap<ViewId, ViewKind> = views.iter().map(|v| (v.id, v.kind)).collect();
    let (dimensions, next_dim) = dimensions_from(origin, d, &kinds)?;
    if d.positionen.len() > MAX_BALLOONS {
        return Err(ctx(tr!(
            "proj-balloons-max",
            max = MAX_BALLOONS.to_string()
        )));
    }
    let next_balloon = counter(
        origin,
        &tr!("proj-w-balloon-counter", name = d.name.as_str()),
        d.naechste_position,
        d.positionen.iter().map(|b| b.id),
    )?;
    let mut balloons = Vec::with_capacity(d.positionen.len());
    for b in &d.positionen {
        balloons.push(Balloon {
            id: BalloonId(b.id),
            view: ViewId(b.ansicht),
            body: body_key_from(b.koerper).ok_or_else(|| ctx(tr!("proj-balloon-bad-body")))?,
            anchor: (b.anker[0], b.anker[1]),
            offset: (b.versatz[0], b.versatz[1]),
        });
    }
    // Fehlt die Einheit (Dateien bis Format 11, Zeichnungen in mm), gilt Millimeter.
    let unit = match d.einheit.as_deref() {
        None => DisplayUnit::Millimeter,
        Some(id) => {
            DisplayUnit::from_id(id).ok_or_else(|| ctx(tr!("proj-unknown-unit", value = id)))?
        }
    };
    let drawing = Drawing {
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
        kind: match d.stuecklistenseite {
            Some(page) => DrawingKind::PartsList { page },
            None => DrawingKind::Views,
        },
        balloons,
        next_balloon,
        unit,
    };
    // Stücklistenblätter und Nummernkreise prüft dieselbe Regel wie beim Anlegen im Dokument;
    // die Meldung nennt die Zeichnung.
    if drawing.balloons.is_empty()
        && drawing.kind == DrawingKind::Views
        && drawing.views.iter().all(|v| v.section.is_none())
    {
        return Ok(drawing);
    }
    validate_drawing(&drawing).map_err(ctx)?;
    Ok(drawing)
}

/// Liest die Zeichnungen und den Zeichnungszähler. Weitere Prüfungen (doppelte Kennungen und
/// Namen, Ausgangsansichten) macht `Document::from_snapshot`.
fn drawings_from(origin: &str, file: &VersionFile) -> Result<(Vec<Drawing>, u64)> {
    if file.format < 4 && (!file.zeichnungen.is_empty() || file.naechste_zeichnung != 0) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-drawings"),
                format = "4"
            ),
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
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-dims"),
                format = "5"
            ),
        ));
    }
    if file.format < 6 && file.zeichnungen.iter().any(|z| z.blattangaben.is_some()) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-sheet"),
                format = "6"
            ),
        ));
    }
    if file.format < 8
        && file
            .zeichnungen
            .iter()
            .any(|z| z.ansichten.iter().any(|v| v.schnitt.is_some()))
    {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-sections"),
                format = "8"
            ),
        ));
    }
    check_len(origin, &|| tr!("proj-l-drawings"), file.zeichnungen.len())?;
    if file.zeichnungen.len() > MAX_DRAWINGS {
        return Err(ProjectError::corrupt(origin, tr!("proj-drawings-too-many")));
    }
    let next = counter(
        origin,
        &tr!("proj-w-drawing-counter"),
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

/// Megabyte für die Meldung, mit dem Dezimaltrenner der aktuellen Sprache.
fn mb_text(bytes: usize) -> String {
    planora_i18n::localize_decimal(&step_text_megabytes(bytes).replace(',', "."))
}

/// JSON-Text einer Versionsdatei. Enthält der Snapshot eine nicht endliche Zahl, ist das ein
/// Fehler (das Dokument wird dann nicht geschrieben).
pub fn encode_version(s: &Snapshot) -> Result<String> {
    let profiles = profile_fields(s);
    let format = format_with(s, &profiles);
    let (step_formen, forms) = step_forms_to(s);
    // Mehr STEP-Text, als sich lesen ließe: eigene Meldung statt „ungültige Werte“. Der Import
    // lässt das nicht zu; die Prüfung hier schützt vor jedem anderen Weg dorthin.
    let total = step_formen
        .iter()
        .fold(0usize, |sum, f| sum.saturating_add(f.text.len()));
    let limit = step_text_sum_limit();
    if total > limit {
        return Err(ProjectError::Invalid(tr!(
            "proj-step-too-large-to-save",
            size = mb_text(total),
            max = mb_text(limit)
        )));
    }
    let bilder = images_to(s)?;
    let file = VersionFile {
        format,
        marker: s.marker,
        naechste_id: s.next_id,
        features: s
            .features
            .iter()
            .zip(profiles)
            .map(|(f, profile)| {
                Ok(FeatureDto {
                    id: f.id.0,
                    name: f.name.clone(),
                    unterdrueckt: f.suppressed,
                    art: kind_to(&f.kind, &forms, profile)?,
                    formeln: f
                        .formulas
                        .iter()
                        .map(|(k, t)| (k.file_key().to_string(), t.clone()))
                        .collect(),
                })
            })
            .collect::<Result<_>>()?,
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
            .map(|d| drawing_to(d, format >= 5, format >= 7))
            .collect::<Result<Vec<_>>>()?,
        // Ohne Zeichnungen fehlt der Zähler, damit alte Dateien unverändert bleiben (F7).
        naechste_zeichnung: if s.drawings.is_empty() {
            0
        } else {
            s.next_drawing
        },
        projektangaben: s.project_info.as_ref().map(project_to),
        skizzengruppen: sketch_groups_to(&s.sketch_groups),
        stueckliste: overrides_to(&s.parts_overrides),
        parameter: s.parameters.iter().map(parameter_to).collect(),
        // Der Zähler steht nur mit Parametern in der Datei (ohne fehlt er, alte Dateien bleiben
        // Byte für Byte); ohne Parameter beginnen Kennungen nach dem Laden wieder bei 1.
        naechster_parameter: if s.parameters.is_empty() {
            0
        } else {
            s.next_param
        },
        rueckrollstellen: s.rollbacks.iter().map(|r| r.0).collect(),
        sichtbarkeit: visibility_to(&s.visibility),
        bilder,
        step_formen,
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|e| ProjectError::Invalid(tr!("proj-write-failed", msg = e.to_string())))?;
    // Nur schreiben, was sich wieder lesen lässt (nicht endliche Zahlen schreibt serde_json
    // als `null`; das fällt beim Lesen auf).
    decode_version(&text, &tr!("proj-origin-output"))
        .map_err(|e| ProjectError::Invalid(tr!("proj-invalid-values", msg = e.to_string())))?;
    Ok(text)
}

/// Inhalt aus Format 16 in einer Feature-Art der Datei (Gegenstück zu [`needs_format_16`]).
fn has_format_16_data(k: &KindDto) -> bool {
    match k {
        KindDto::Konstruktionsebene(_) => true,
        KindDto::Skizze(s) => {
            matches!(s.lage, PlaneDto::Ebene { .. })
                || s.kopien
                    .iter()
                    .any(|c| c.schnitt.is_some() || c.silhouette.is_some())
        }
        _ => false,
    }
}

/// Inhalt aus Format 17 in einer Feature-Art der Datei (Gegenstück zu [`needs_format_17`]).
fn has_format_17_data(k: &KindDto) -> bool {
    matches!(k, KindDto::Skizze(s) if s
    .skizze
    .kurven
    .iter()
    .any(|c| match &c.art {
        CurveKindDto::KontrollSpline { .. }
        | CurveKindDto::Kegelschnitt { .. }
        | CurveKindDto::FesterSpline { .. } => true,
        CurveKindDto::Spline { griffe, .. } => !griffe.is_empty(),
        _ => false,
    }) || s
        .skizze
        .abhaengigkeiten
        .iter()
        .any(|c| matches!(c.art, ConstraintKindDto::KruemmungsGriff { .. })))
}

/// Inhalt aus Format 18 in einer Feature-Art der Datei (Gegenstück zu [`needs_format_18`]).
fn has_format_18_data(k: &KindDto) -> bool {
    matches!(k, KindDto::Skizze(s) if !s.skizze.texte.is_empty())
}

/// Kontrollpunktrahmen an einer Skizzenkurve der Datei (Gegenstück zu [`needs_format_19`]).
fn has_control_frame(k: &KindDto) -> bool {
    matches!(k, KindDto::Skizze(s) if s.skizze.kurven.iter().any(|c| c.rahmen))
}

/// Inhalt aus Format 21 in einer Feature-Art der Datei (Gegenstück zu [`needs_format_21`]):
/// Kamm, Gegenwinkel, Tangente am Ende oder Glatt an einem Ellipsenbogen.
fn has_format_21_data(k: &KindDto) -> bool {
    let KindDto::Skizze(s) = k else {
        return false;
    };
    let s = &s.skizze;
    let elliptic = |i: usize| {
        s.kurven
            .get(i)
            .is_some_and(|c| matches!(c.art, CurveKindDto::Ellipsenbogen { .. }))
    };
    s.kurven.iter().any(|c| c.kamm.is_some())
        || s.abhaengigkeiten.iter().any(|c| {
            c.gegenwinkel
                || match &c.art {
                    ConstraintKindDto::TangenteEnde { .. } => true,
                    ConstraintKindDto::Glatt { a, b, .. } => elliptic(*a) || elliptic(*b),
                    _ => false,
                }
        })
}

/// Inhalt aus Format 22 in einer Feature-Art der Datei (Gegenstück zu [`needs_format_22`]):
/// Höhenformel, `{Name}` in einem Text (auch gelöscht), Drehprofil oder Isokurve. Die
/// Skizzengruppen prüft [`decode_version`] an der Datei.
fn has_format_22_data(k: &KindDto) -> bool {
    let KindDto::Skizze(s) = k else {
        return false;
    };
    !s.hoehenformeln.is_empty()
        || s.skizze.texte.iter().any(|t| has_param_ref(&t.inhalt))
        || s.kopien
            .iter()
            .any(|c| c.drehprofil.is_some() || c.isokurve.is_some())
}

/// 3D-Geometrie in einer Skizze der Datei (Gegenstück zu [`needs_format_20`]).
fn has_format_20_data(k: &KindDto) -> bool {
    let KindDto::Skizze(s) = k else {
        return false;
    };
    let copies_20 = s.kopien.iter().any(|c| {
        c.punkt.is_some()
            || c.stueck != 0
            || c.stuecke > 1
            || c.einbezug.is_some()
            || c.skizzenkurve.is_some()
            || c.auf_flaeche.is_some()
            || c.schnittkurve.is_some()
    });
    let s = &s.skizze;
    copies_20
        || s.drei_d
        || s.punkte.iter().any(|p| p.z.is_some())
        || s.texte.iter().any(|t| t.normale.is_some())
        || s.kurven.iter().any(|c| {
            c.normale.is_some()
                || match &c.art {
                    CurveKindDto::Ellipse { az, .. } | CurveKindDto::Ellipsenbogen { az, .. } => {
                        az.is_some()
                    }
                    CurveKindDto::FesterSpline { hoehen, .. } => !hoehen.is_empty(),
                    _ => false,
                }
        })
        || s.abhaengigkeiten.iter().any(|c| {
            matches!(
                c.art,
                ConstraintKindDto::EntlangZ { .. }
                    | ConstraintKindDto::Fest3 { .. }
                    | ConstraintKindDto::InEbene { .. }
                    | ConstraintKindDto::Ellipsenachsen3 { .. }
            )
        })
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
        return Err(ProjectError::corrupt(origin, tr!("proj-format-zero")));
    }
    if head.format > FORMAT_VERSION {
        return Err(ProjectError::TooNew {
            path: origin.into(),
            found: head.format,
        });
    }
    let mut file: VersionFile =
        serde_json::from_str(text).map_err(|e| ProjectError::corrupt(origin, e.to_string()))?;
    if file.format < 2
        && file
            .features
            .iter()
            .any(|f| matches!(f.art, KindDto::Verrunden { .. }))
    {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-blends"),
                format = "2"
            ),
        ));
    }
    let has_mesh_body = file
        .features
        .iter()
        .any(|f| matches!(f.art, KindDto::NetzKoerper { .. }));
    if file.format < 3 && (has_mesh_body || !file.netze.is_empty()) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-mesh"),
                format = "3"
            ),
        ));
    }
    if file.format < 6 && file.projektangaben.is_some() {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-sheet"),
                format = "6"
            ),
        ));
    }
    let has_parts = |z: &ZeichnungDto| {
        z.stuecklistenseite.is_some()
            || !z.positionen.is_empty()
            || z.naechste_position != 0
            || z.blattangaben.as_ref().is_some_and(|b| b.stueckliste)
    };
    if file.format < 7 && (!file.stueckliste.is_empty() || file.zeichnungen.iter().any(has_parts)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-parts"),
                format = "7"
            ),
        ));
    }
    if file.format < 9 && has_format_9_data(&file) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-params"),
                format = "9"
            ),
        ));
    }
    let has_step_body = file
        .features
        .iter()
        .any(|f| matches!(f.art, KindDto::StepKoerper { .. }));
    if file.format < 10 && (has_step_body || !file.step_formen.is_empty()) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-step"),
                format = "10"
            ),
        ));
    }
    let has_profile_refs = file.features.iter().any(|f| {
        matches!(
            f.art,
            KindDto::Extrusion {
                profile: Some(_),
                ..
            } | KindDto::Rotation {
                profile: Some(_),
                ..
            }
        )
    });
    if file.format < 13 && has_profile_refs {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-profile-refs"),
                format = "13"
            ),
        ));
    }
    if file.format < 16 && file.features.iter().any(|f| has_format_16_data(&f.art)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-planes"),
                format = "16"
            ),
        ));
    }
    if file.format < 16 && file.sichtbarkeit.is_some() {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-visibility"),
                format = "16"
            ),
        ));
    }
    if file.format < 17 && file.features.iter().any(|f| has_format_17_data(&f.art)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-control-spline"),
                format = "17"
            ),
        ));
    }
    if file.format < 18 && file.features.iter().any(|f| has_format_18_data(&f.art)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-text"),
                format = "18"
            ),
        ));
    }
    let has_canvas = file
        .features
        .iter()
        .any(|f| matches!(f.art, KindDto::Leinwand(_)));
    let canvas_folder = file
        .sichtbarkeit
        .as_ref()
        .is_some_and(|v| v.ordner.contains(&OrdnerDto::Leinwaende));
    if file.format < 19 && (has_canvas || !file.bilder.is_empty() || canvas_folder) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-canvas"),
                format = "19"
            ),
        ));
    }
    if file.format < 19 && file.features.iter().any(|f| has_control_frame(&f.art)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-control-frame"),
                format = "19"
            ),
        ));
    }
    if file.format < 20 && file.features.iter().any(|f| has_format_20_data(&f.art)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-3d-sketch"),
                format = "20"
            ),
        ));
    }
    if file.format < 21 && file.features.iter().any(|f| has_format_21_data(&f.art)) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-sketch-check"),
                format = "21"
            ),
        ));
    }
    if file.format < 22
        && (!file.skizzengruppen.is_empty()
            || file.features.iter().any(|f| has_format_22_data(&f.art)))
    {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-sketch-transfer"),
                format = "22"
            ),
        ));
    }
    let visibility = visibility_from(file.sichtbarkeit.as_ref());
    let sketch_groups = sketch_groups_from(origin, &file)?;
    let rollbacks = rollbacks_from(origin, &file)?;
    let (parameters, next_param) = parameters_from(origin, &file)?;
    let parts_overrides = overrides_from(origin, &file.stueckliste)?;
    let (drawings, next_drawing) = drawings_from(origin, &file)?;
    let project_info = file
        .projektangaben
        .as_ref()
        .map(|p| project_from(origin, p))
        .transpose()?;
    check_len(origin, &|| tr!("proj-l-meshes"), file.netze.len())?;
    let meshes = meshes_from(origin, &file.netze)?;
    for f in &file.features {
        if let KindDto::NetzKoerper { netz, .. } = &f.art
            && !meshes.contains_key(&MeshId(*netz))
        {
            return Err(ProjectError::corrupt(
                origin,
                tr!(
                    "proj-ctx-feature",
                    name = f.name.as_str(),
                    msg = tr!("proj-mesh-missing", id = netz.to_string())
                ),
            ));
        }
    }
    let step_shapes = step_forms_from(origin, std::mem::take(&mut file.step_formen))?;
    for f in &file.features {
        if let KindDto::StepKoerper { form } = &f.art
            && !step_shapes.contains_key(&StepShapeId(*form))
        {
            return Err(ProjectError::corrupt(
                origin,
                tr!(
                    "proj-ctx-feature",
                    name = f.name.as_str(),
                    msg = tr!("proj-step-form-missing", id = form.to_string())
                ),
            ));
        }
    }
    check_len(origin, &|| tr!("proj-l-images"), file.bilder.len())?;
    let images = images_from(origin, &std::mem::take(&mut file.bilder))?;
    for f in &file.features {
        if let KindDto::Leinwand(c) = &f.art
            && !images.contains_key(&ImageId(c.bild))
        {
            return Err(ProjectError::corrupt(
                origin,
                tr!(
                    "proj-ctx-feature",
                    name = f.name.as_str(),
                    msg = tr!("proj-image-missing", id = c.bild.to_string())
                ),
            ));
        }
    }
    check_len(origin, &|| tr!("proj-l-features"), file.features.len())?;
    let mut features = Vec::with_capacity(file.features.len());
    let mut dissolved_patterns = 0;
    for f in &file.features {
        let kind = kind_from(origin, &f.name, &f.art, &mut dissolved_patterns)?;
        let formulas = feature_formulas_from(origin, f, &kind)?;
        features.push(SnapshotFeature {
            id: FeatureId(f.id),
            name: f.name.clone(),
            suppressed: f.unterdrueckt,
            kind,
            formulas,
        });
    }
    migrate_profiles(origin, &mut features)?;
    // Eine Kopie mit fehlender oder dahinterstehender Quelle gibt es erst ab Format 21 (I-2).
    if file.format < 21 && has_lost_source(&features) {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-lost-source"),
                format = "21"
            ),
        ));
    }
    Ok(Snapshot {
        features,
        marker: file.marker,
        next_id: file.naechste_id,
        meshes,
        step_shapes,
        drawings,
        next_drawing,
        project_info,
        parts_overrides,
        visibility,
        parameters,
        next_param,
        rollbacks,
        images,
        dissolved_patterns,
        sketch_groups,
    })
}

/// Liest die Rückrollstellen (ab Format 11). Prüft hier nur, dass jede Stelle einen Schritt der
/// Datei nennt; dass keine Stelle im Verlauf einer anderen liegt, prüft
/// `Document::from_snapshot` (es braucht die Ketten).
fn rollbacks_from(origin: &str, file: &VersionFile) -> Result<Vec<FeatureId>> {
    if file.format < 11 && !file.rueckrollstellen.is_empty() {
        return Err(ProjectError::corrupt(
            origin,
            tr!(
                "proj-since-format",
                what = tr!("proj-feat-rollbacks"),
                format = "11"
            ),
        ));
    }
    check_len(
        origin,
        &|| tr!("proj-l-rollbacks"),
        file.rueckrollstellen.len(),
    )?;
    let ids: HashSet<u64> = file.features.iter().map(|f| f.id).collect();
    file.rueckrollstellen
        .iter()
        .map(|&r| {
            if ids.contains(&r) {
                Ok(FeatureId(r))
            } else {
                Err(ProjectError::corrupt(
                    origin,
                    tr!("proj-rollback-unknown", id = r.to_string()),
                ))
            }
        })
        .collect()
}

#[cfg(test)]
mod image_limit_tests {
    //! Gesamtgrenze der Bildbytes (Gesamtprüfung I5) mit kleiner Grenze statt 1 GiB.
    use super::*;
    use planora_feature::Document;

    fn image(n: usize, fill: u8) -> Arc<CanvasImage> {
        let bytes: Arc<[u8]> = vec![fill; n].into();
        Arc::new(CanvasImage {
            hash: image_hash(&bytes),
            bytes,
            format: ImageFormat::Png,
            width: 1,
            height: 1,
        })
    }

    #[test]
    fn the_constant_is_one_gib() {
        assert_eq!(MAX_IMAGES_BYTES, 1024 * 1024 * 1024);
    }

    #[test]
    fn saving_refuses_images_over_the_total_limit() {
        let mut s = Document::new().snapshot();
        s.images.insert(ImageId(1), image(60, 1));
        s.images.insert(ImageId(2), image(60, 2));
        assert!(images_to_limited(&s, 120).is_ok());
        let e = images_to_limited(&s, 119).unwrap_err();
        assert!(e.to_string().contains("MB"), "{e}");
    }

    #[test]
    fn loading_refuses_images_over_the_total_limit_before_decoding() {
        let dto = |id: u64| ImageDto {
            id,
            format: "png".into(),
            daten: base64::encode(&[7u8; 60]),
        };
        let list = [dto(1), dto(2)];
        // Geprüft wird je Bild vor dem Entschlüsseln: schon das erste liegt über 59 Bytes.
        let over = images_from_limited("t", &list, 59).unwrap_err().to_string();
        assert!(over.contains(&tr!("proj-images-too-big")), "{over}");
        // Innerhalb der Grenze kommt der Inhalt dran (hier kein echtes PNG).
        let inside = images_from_limited("t", &list, 120)
            .unwrap_err()
            .to_string();
        assert!(!inside.contains(&tr!("proj-images-too-big")), "{inside}");
    }
}
