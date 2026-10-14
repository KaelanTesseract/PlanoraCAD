//! Berechnung eines einzelnen Features aus seinen Parametern und den sichtbaren Körpern.

use crate::error::FeatureError;
use crate::model::{
    BlendArt, BodyRef, Document, EdgeCopy, FeatureId, FeatureKind, MAX_MESH_SCALE, MIN_MESH_SCALE,
    ROTATION_NORM_TOL, SketchFeature, SketchPlane,
};
use crate::refs::{FaceRef, first_face, resolve_edge, resolve_face};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_kernel::blend::{MAX_BLEND_EDGES, MAX_RADIUS_POINTS};
use planora_kernel::boolean::BooleanError;
use planora_kernel::{
    BlendEdge, BlendError, BlendRequest, Blended, BoolOp, Kernel, Solid, SurfaceKind,
};
use planora_math::{DMat4, DVec3};
use planora_mesh::{
    BLEND_REL, BLEND_SLACK_FACTOR, Discretizer, TessellationOptions, blend_volume_plausible,
    boolean_volume_plausible, curved_area, mass_properties, tessellate, tessellate_with,
};
use planora_sketch::{Sketch, extrude_profile, plane_of_face, revolve_profile};

/// Größter Betrag einer Koordinate oder eines Maßes in mm (wie
/// [`planora_sketch::model::MAX_COORD`]): Darüber ist der Abstand benachbarter `f64`-Werte größer
/// als die Toleranz, Toleranzvergleiche würden bedeutungslos und die Tessellierung bricht
/// zusammen. Maße, Lagen, Verschiebungen und alle Ergebniskörper werden dagegen geprüft.
pub const MAX_COORD_MM: f64 = planora_sketch::model::MAX_COORD;

/// Spielraum der Volumenprobe relativ zur Summe der Eingangsvolumen (wie in der App).
const VOLUME_REL_TOL: f64 = 0.02;

/// Nach der Auflösung aktualisierte Verweise einer Skizze (Hinweispunkte).
#[derive(Clone, Debug)]
pub(crate) struct SketchHints {
    pub(crate) plane: SketchPlane,
    pub(crate) copies: Vec<EdgeCopy>,
}

/// Ergebnis der Berechnung eines Features.
#[derive(Clone, Debug, Default)]
pub(crate) struct Computed {
    pub(crate) bodies: Vec<Solid>,
    pub(crate) sketch: Option<Sketch>,
    pub(crate) hints: Option<SketchHints>,
}

impl Computed {
    fn bodies(bodies: Vec<Solid>) -> Computed {
        Computed {
            bodies,
            ..Computed::default()
        }
    }
}

/// Berechnet das Feature an Position `idx` aus den Körpern `visible` (sichtbar vor ihm).
pub(crate) fn compute_feature(
    doc: &Document,
    idx: usize,
    visible: &[BodyRef],
) -> Result<Computed, FeatureError> {
    check_parameters(&doc.features[idx].kind)?;
    let computed = compute_unchecked(doc, idx, visible)?;
    for body in &computed.bodies {
        check_extent(body)?;
    }
    Ok(computed)
}

/// Endlich und betragsmäßig höchstens [`MAX_COORD_MM`]?
fn in_range(v: f64) -> bool {
    v.is_finite() && v.abs() <= MAX_COORD_MM
}

/// Prüft die Zahlen eines Features gegen [`MAX_COORD_MM`], bevor der Kernel rechnet. Die
/// Ausdehnung eines Grundkörpers (Lage plus Maß) muss ebenfalls im Bereich liegen.
pub(crate) fn check_parameters(kind: &FeatureKind) -> Result<(), FeatureError> {
    let out = |what: &str| Err(FeatureError::OutOfRange(what.to_string()));
    let place = |origin: DVec3, extent: f64| {
        origin
            .to_array()
            .iter()
            .all(|c| in_range(*c) && in_range(c.abs() + extent))
    };
    match kind {
        FeatureKind::Box { size, origin } => {
            if !size.to_array().iter().all(|v| in_range(*v)) {
                return out("Die Größe des Quaders");
            }
            if !place(*origin, size.abs().max_element()) {
                return out("Der Quader");
            }
        }
        FeatureKind::Cylinder {
            radius,
            height,
            origin,
        }
        | FeatureKind::Cone {
            radius,
            height,
            origin,
        } => {
            if !(in_range(*radius) && in_range(*height)) {
                return out("Radius oder Höhe");
            }
            if !place(*origin, radius.abs().max(height.abs())) {
                return out("Der Körper");
            }
        }
        FeatureKind::Sphere { radius, origin } => {
            if !in_range(*radius) {
                return out("Der Radius");
            }
            if !place(*origin, radius.abs()) {
                return out("Die Kugel");
            }
        }
        FeatureKind::Extrude { height, .. } => {
            if !in_range(*height) {
                return out("Die Höhe der Extrusion");
            }
        }
        FeatureKind::Move { delta, .. } => {
            if !delta.to_array().iter().all(|v| in_range(*v)) {
                return out("Die Verschiebung");
            }
        }
        FeatureKind::Verrunden { edges, art, .. } => {
            if edges.is_empty() {
                return Err(FeatureError::InvalidInput(
                    "Es ist keine Kante gewählt".into(),
                ));
            }
            // Grenzen des Kernels schon hier: Eine feindliche Datei mit sehr vielen Kanten oder
            // Radienpunkten soll nicht erst jede Kante im Körper suchen lassen.
            if edges.len() > MAX_BLEND_EDGES {
                return Err(FeatureError::InvalidInput(format!(
                    "Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"
                )));
            }
            if let BlendArt::RundungVariabel { punkte } = art
                && punkte.len() > MAX_RADIUS_POINTS
            {
                return Err(FeatureError::InvalidInput(format!(
                    "Ein variabler Radius braucht 2 bis {MAX_RADIUS_POINTS} Punkte"
                )));
            }
            if !art.numbers().iter().all(|v| in_range(*v)) {
                return out("Ein Maß der Rundung oder Fase");
            }
        }
        FeatureKind::MeshBody {
            offset,
            rotation,
            scale,
            ..
        } => {
            if !offset.to_array().iter().all(|v| in_range(*v)) {
                return out("Die Position des Netz-Körpers");
            }
            if !(scale.is_finite() && (MIN_MESH_SCALE..=MAX_MESH_SCALE).contains(scale)) {
                return Err(FeatureError::InvalidInput(
                    "Der Maßstab muss zwischen 0,000001 und 1 000 000 liegen".into(),
                ));
            }
            if !(rotation.is_finite() && (rotation.length() - 1.0).abs() <= ROTATION_NORM_TOL) {
                return Err(FeatureError::InvalidInput(
                    "Die Drehung des Netz-Körpers ist ungültig".into(),
                ));
            }
        }
        FeatureKind::Sketch(_) | FeatureKind::Revolve { .. } | FeatureKind::Boolean { .. } => {}
    }
    Ok(())
}

/// Alle Eckpunkte des Ergebniskörpers liegen im Bereich ±[`MAX_COORD_MM`] (deckt auch
/// verschobene und aus Skizzen erzeugte Körper ab).
fn check_extent(solid: &Solid) -> Result<(), FeatureError> {
    if solid
        .vertices
        .iter()
        .all(|(_, v)| v.point.to_array().iter().all(|c| in_range(*c)))
    {
        Ok(())
    } else {
        Err(FeatureError::OutOfRange("Der Körper".into()))
    }
}

/// Alle Punkte des Netzes liegen nach der Lage `t` im Bereich ±[`MAX_COORD_MM`].
pub(crate) fn check_mesh_extent(
    mesh: &ExchangeMesh,
    t: &MeshTransform,
) -> Result<(), FeatureError> {
    let inside = (0..mesh.positions.len()).all(|i| {
        t.apply(mesh.point(i))
            .to_array()
            .iter()
            .all(|c| in_range(*c))
    });
    if inside {
        Ok(())
    } else {
        Err(FeatureError::OutOfRange("Der Netz-Körper".into()))
    }
}

/// Netz-Körper lassen sich nicht weiterverarbeiten (Spec §6, Bearbeitungssperre).
fn ensure_not_mesh(doc: &Document, id: FeatureId) -> Result<(), FeatureError> {
    match doc.feature(id).map(|f| &f.kind) {
        Some(FeatureKind::MeshBody { .. }) => Err(FeatureError::MeshBodyLocked),
        _ => Ok(()),
    }
}

/// Berechnung ohne die Bereichsprüfung (siehe [`compute_feature`]).
fn compute_unchecked(
    doc: &Document,
    idx: usize,
    visible: &[BodyRef],
) -> Result<Computed, FeatureError> {
    let feature = &doc.features[idx];
    let id = feature.id;
    let kernel = &doc.kernel;
    match &feature.kind {
        FeatureKind::Sketch(sf) => compute_sketch(doc, sf, visible),
        FeatureKind::Box { size, origin } => primitive(doc, kernel.make_box(*size), *origin, id.0),
        FeatureKind::Cylinder {
            radius,
            height,
            origin,
        } => primitive(doc, kernel.make_cylinder(*radius, *height), *origin, id.0),
        FeatureKind::Sphere { radius, origin } => {
            primitive(doc, kernel.make_sphere(*radius), *origin, id.0)
        }
        FeatureKind::Cone {
            radius,
            height,
            origin,
        } => primitive(doc, kernel.make_cone(*radius, *height), *origin, id.0),
        FeatureKind::Extrude {
            sketch,
            profile,
            height,
        } => {
            let sk = input_sketch(doc, *sketch)?;
            let p = nth_profile(sk, *profile)?;
            let solid = extrude_profile(kernel, sk, &p, *height).map_err(sketch_error)?;
            finish(doc, solid, id.0)
        }
        FeatureKind::Revolve {
            sketch,
            profile,
            axis,
        } => {
            let sk = input_sketch(doc, *sketch)?;
            let p = nth_profile(sk, *profile)?;
            let solid = revolve_profile(kernel, sk, &p, *axis).map_err(sketch_error)?;
            finish(doc, solid, id.0)
        }
        FeatureKind::Boolean { target, tool, op } => {
            let a = visible_body(doc, visible, *target)?;
            let b = visible_body(doc, visible, *tool)?;
            if target == tool {
                return Err(FeatureError::InvalidInput(
                    "Ziel und Werkzeug müssen verschiedene Körper sein".into(),
                ));
            }
            let mut result = kernel
                .boolean(a, b, *op)
                .map_err(|e| boolean_error(a, b, &e))?;
            check_volume(doc, *op, a, b, &result)?;
            result.assign_origin(id.0);
            Ok(Computed::bodies(vec![result]))
        }
        FeatureKind::Move { body, delta } => {
            let s = visible_body(doc, visible, *body)?;
            let moved = kernel
                .transform(s, &DMat4::from_translation(*delta))
                .map_err(|e| FeatureError::Kernel(e.to_string()))?;
            Ok(Computed::bodies(vec![moved]))
        }
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => {
            let s = visible_body(doc, visible, *body)?;
            let tol = kernel.tolerance;
            let mut list = Vec::with_capacity(edges.len());
            for r in edges {
                let found = resolve_edge(&[s], r, &tol)?;
                list.push(BlendEdge {
                    edge: found.edge,
                    first_face: first_face(s, found.edge, &r.a),
                });
            }
            let request = BlendRequest {
                edges: list,
                kind: art.kernel_kind(),
                tangent_chain: *tangentenkette,
            };
            let blended = kernel.blend(s, &request).map_err(|e| blend_error(s, &e))?;
            check_blend_volume(doc, s, &blended)?;
            let mut result = blended.solid;
            result.assign_origin(id.0);
            Ok(Computed::bodies(vec![result]))
        }
        FeatureKind::MeshBody { mesh, .. } => {
            let m = doc
                .meshes
                .get(mesh)
                .ok_or(FeatureError::MeshMissing(mesh.0))?;
            let t = feature.kind.mesh_transform().unwrap_or_default();
            check_mesh_extent(m, &t)?;
            Ok(Computed::default())
        }
    }
}

/// Fehlermeldung einer gescheiterten Booleschen Operation. Ist ein Eingangskörper mit
/// Freiformflächen im Spiel und stammt die Ursache aus einem Freiform-Weg des Kernels
/// ([`BooleanError::is_freeform_limit`]: Schnittrechner, Einordnen gegen Freiformflächen, dünne
/// Gebilde, Arbeitsgrenze), beginnt die Meldung mit „Bei Körpern mit Freiformflächen:“ und
/// schlägt eine kleine Änderung vor. `AmbiguousContact` und `AmbiguousClassification` entstehen
/// in der gemeinsamen Pipeline; mit Freiformbeteiligung bekommen sie den Hinweis ebenfalls (die
/// Pipeline rechnet dann mit der weiteren Freiform-Toleranz, die Ursache lässt sich nicht
/// trennen). Alle übrigen Fehler (etwa Ebene gegen Kegel, leeres oder ungültiges Ergebnis)
/// behalten ihren Text, ebenso alles bei rein analytischen Körpern. Der Vorschlag folgt der Form
/// der übrigen Meldungen („…; bitte …“) und entfällt, wenn die Ursache schon einen enthält.
fn boolean_error(a: &Solid, b: &Solid, e: &BooleanError) -> FeatureError {
    let freeform_body = is_freeform(a) || is_freeform(b);
    let hinted = e.is_freeform_limit()
        || matches!(
            e,
            BooleanError::AmbiguousContact | BooleanError::AmbiguousClassification
        );
    if !(freeform_body && hinted) {
        return FeatureError::Kernel(e.to_string());
    }
    let cause = e.to_string();
    if cause.contains("; bitte ") {
        FeatureError::Kernel(format!("Bei Körpern mit Freiformflächen: {cause}"))
    } else {
        FeatureError::Kernel(format!(
            "Bei Körpern mit Freiformflächen: {cause}; bitte Lage oder Maße eines Körpers leicht ändern"
        ))
    }
}

/// Hat der Körper eine Freiformfläche?
fn is_freeform(s: &Solid) -> bool {
    s.faces
        .iter()
        .any(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
}

/// Meldung einer gescheiterten Verrundung (P11): bei Freiformkörpern und Ursachen aus der Bahn
/// mit dem bekannten Vorsatz.
fn blend_error(input: &Solid, e: &BlendError) -> FeatureError {
    let limit = matches!(
        e,
        BlendError::SpineLost | BlendError::Unsupported(_) | BlendError::TooExpensive
    );
    if is_freeform(input) && limit {
        FeatureError::Kernel(format!("Bei Körpern mit Freiformflächen: {e}"))
    } else {
        FeatureError::Kernel(e.to_string())
    }
}

/// Volumenprobe einer Verrundung (Spec §4.5, P2): Eingang und Ergebnis mit gemeinsamer Teilung
/// vernetzen und die Änderung mit der Schätzung des Kernels vergleichen (Spielräume
/// [`BLEND_REL`] relativ zur Summe der Beträge `volume_scale` und [`BLEND_SLACK_FACTOR`] aus
/// `planora-mesh`, gemeinsam mit dessen Tests).
fn check_blend_volume(
    doc: &Document,
    input: &Solid,
    blended: &Blended,
) -> Result<(), FeatureError> {
    let disc = Discretizer::for_solids(&[input, &blended.solid], doc.chord_tolerance);
    let mesh =
        |s: &Solid| tessellate_with(s, &disc).map_err(|e| FeatureError::Kernel(e.to_string()));
    let (mi, mo) = (mesh(input)?, mesh(&blended.solid)?);
    let slack = BLEND_SLACK_FACTOR
        * disc.chord_tolerance()
        * (curved_area(input, &mi) + curved_area(&blended.solid, &mo));
    if blend_volume_plausible(
        mass_properties(&mi).volume,
        mass_properties(&mo).volume,
        blended.volume_change,
        blended.volume_scale,
        BLEND_REL,
        slack,
    ) {
        Ok(())
    } else {
        Err(FeatureError::ImplausibleBlend)
    }
}

/// Grundkörper: Kernel-Ergebnis, verschoben um `origin`, geprüft und markiert.
fn primitive(
    doc: &Document,
    made: Result<Solid, planora_kernel::KernelError>,
    origin: DVec3,
    id: u64,
) -> Result<Computed, FeatureError> {
    let mut solid = made.map_err(|e| FeatureError::Kernel(e.to_string()))?;
    if origin != DVec3::ZERO {
        solid = doc
            .kernel
            .transform(&solid, &DMat4::from_translation(origin))
            .map_err(|e| FeatureError::Kernel(e.to_string()))?;
    }
    finish(doc, solid, id)
}

/// Prüft den Körper mit dem Validator und weist den noch freien Marken die Herkunft zu.
fn finish(doc: &Document, mut solid: Solid, id: u64) -> Result<Computed, FeatureError> {
    doc.kernel
        .validate(&solid)
        .map_err(|e| FeatureError::Kernel(e.to_string()))?;
    solid.assign_origin(id);
    Ok(Computed::bodies(vec![solid]))
}

fn sketch_error(e: planora_sketch::SketchError) -> FeatureError {
    FeatureError::Sketch(e.to_string())
}

/// Berechnete Skizze des Eingangsfeatures einer Extrusion oder Rotation.
fn input_sketch(doc: &Document, id: crate::model::FeatureId) -> Result<&Sketch, FeatureError> {
    doc.sketch_of(id).ok_or_else(|| {
        let name = doc.feature(id).map_or_else(String::new, |f| f.name.clone());
        FeatureError::InvalidInput(format!("Feature „{name}“ ist keine Skizze"))
    })
}

/// Profil `n` der Skizze. Gibt es das nicht, nennt die Meldung die Gründe der abgelehnten
/// Kurvenzüge (etwa eine zu kleine Ellipse), damit die Ursache sichtbar bleibt.
fn nth_profile(sk: &Sketch, n: usize) -> Result<planora_kernel::Profile, FeatureError> {
    let extracted = sk.profiles();
    if let Some(p) = extracted.profiles.into_iter().nth(n) {
        return Ok(p);
    }
    let mut reasons: Vec<String> = Vec::new();
    for r in &extracted.rejected {
        let text = r.reason.to_string();
        if !reasons.contains(&text) {
            reasons.push(text);
        }
    }
    Err(FeatureError::InvalidInput(if reasons.is_empty() {
        format!("Profil {n} gibt es nicht")
    } else {
        format!(
            "Profil {n} gibt es nicht; abgelehnte Kurvenzüge der Skizze: {}",
            reasons.join("; ")
        )
    }))
}

/// Der Körper `r`, sofern er vor dem Feature sichtbar ist.
fn visible_body<'a>(
    doc: &'a Document,
    visible: &[BodyRef],
    r: BodyRef,
) -> Result<&'a Solid, FeatureError> {
    ensure_not_mesh(doc, r.feature)?;
    let not_found = FeatureError::BodyNotFound {
        feature: r.feature.0,
        index: r.index,
    };
    if !visible.contains(&r) {
        return Err(not_found);
    }
    doc.body(r).ok_or(not_found)
}

/// Volumen über die Tessellierung bei der Sehnentoleranz des Dokuments.
fn volume(doc: &Document, s: &Solid) -> Result<f64, FeatureError> {
    let options = TessellationOptions {
        chord_tolerance: doc.chord_tolerance,
    };
    let mesh = tessellate(s, &options).map_err(|e| FeatureError::Kernel(e.to_string()))?;
    Ok(mass_properties(&mesh).volume)
}

fn check_volume(
    doc: &Document,
    op: BoolOp,
    a: &Solid,
    b: &Solid,
    result: &Solid,
) -> Result<(), FeatureError> {
    // Mit Freiformflächen werden alle drei Körper mit derselben Teilung vernetzt: Der
    // Winkelschritt hängt vom größten Radius des Körpers ab, und die Diagonale einer
    // Freiformfläche macht ihn im Ergebnis feiner als bei einem kleinen Eingangskörper (eine
    // Kugel allein grob, im Ergebnis fein vernetzt: Ergebnis größer als die Summe). Rein
    // analytische Körper: unverändert je Körper.
    if is_freeform(a) || is_freeform(b) {
        let disc = Discretizer::for_solids(&[a, b, result], doc.chord_tolerance);
        let vol = |s: &Solid| -> Result<f64, FeatureError> {
            let mesh =
                tessellate_with(s, &disc).map_err(|e| FeatureError::Kernel(e.to_string()))?;
            Ok(mass_properties(&mesh).volume)
        };
        let (va, vb, v) = (vol(a)?, vol(b)?, vol(result)?);
        return if boolean_volume_plausible(op, va, vb, v, VOLUME_REL_TOL) {
            Ok(())
        } else {
            Err(FeatureError::ImplausibleFreeformResult)
        };
    }
    let (va, vb, v) = (volume(doc, a)?, volume(doc, b)?, volume(doc, result)?);
    if boolean_volume_plausible(op, va, vb, v, VOLUME_REL_TOL) {
        Ok(())
    } else {
        Err(FeatureError::ImplausibleResult)
    }
}

/// Skizze: Ebene auflösen, Kantenkopien nachführen, lösen.
fn compute_sketch(
    doc: &Document,
    sf: &SketchFeature,
    visible: &[BodyRef],
) -> Result<Computed, FeatureError> {
    let solids: Vec<&Solid> = visible.iter().filter_map(|r| doc.body(*r)).collect();
    let tol = doc.kernel.tolerance;
    if let SketchPlane::OnFace(r) = &sf.plane {
        ensure_not_mesh(doc, FeatureId(r.origin))?;
    }
    for c in &sf.copies {
        ensure_not_mesh(doc, FeatureId(c.edge.a.origin))?;
        ensure_not_mesh(doc, FeatureId(c.edge.b.origin))?;
    }
    let mut sketch = sf.sketch.clone();
    let (frame, plane) = match &sf.plane {
        SketchPlane::Standard(p) => (p.frame(), sf.plane.clone()),
        SketchPlane::OnFace(r) => {
            let found = resolve_face(&solids, r, &tol)?;
            let frame = plane_of_face(solids[found.body], found.face).ok_or_else(|| {
                FeatureError::InvalidInput("Die Fläche für die Skizze ist nicht eben".into())
            })?;
            let updated = FaceRef {
                hint: found.hint,
                ..r.clone()
            };
            (frame, SketchPlane::OnFace(updated))
        }
    };
    sketch.set_frame(frame).map_err(sketch_error)?;
    let mut copies = Vec::with_capacity(sf.copies.len());
    for c in &sf.copies {
        let found = resolve_edge(&solids, &c.edge, &tol)?;
        sketch
            .reproject_edge(c.curve, solids[found.body], found.edge)
            .map_err(sketch_error)?;
        let mut updated = c.clone();
        updated.edge.hint = found.hint;
        copies.push(updated);
    }
    // Der Lösungsstatus wird bewusst nicht ausgewertet: eine unterbestimmte Skizze ist gültig.
    let _ = sketch.solve();
    Ok(Computed {
        bodies: Vec::new(),
        sketch: Some(sketch),
        hints: Some(SketchHints { plane, copies }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nur die Grenzen des Rechners bekommen den Freiform-Hinweis; andere Kernfehler behalten
    /// ihren Text, auch wenn ein Freiformkörper beteiligt ist.
    #[test]
    fn only_limit_errors_get_the_freeform_hint() {
        use planora_kernel::{Profile, ProfileLoop};
        use planora_math::nurbs::NurbsCurve;
        use planora_math::{Frame, Tolerance};
        let tol = Tolerance::default();
        let ellipse = NurbsCurve::elliptic_arc(
            DVec3::ZERO,
            DVec3::new(5.0, 0.0, 0.0),
            DVec3::new(0.0, 3.0, 0.0),
            0.0,
            std::f64::consts::TAU,
        )
        .unwrap();
        let profile = Profile {
            outer: ProfileLoop::nurbs_loop(ellipse).unwrap(),
            holes: vec![],
        };
        let frame = Frame::from_origin_normal(DVec3::ZERO, DVec3::Z);
        let freeform = planora_kernel::extrude(&profile, &frame, 4.0, &tol).unwrap();
        let plain = planora_kernel::primitives::make_box(DVec3::ONE, &tol).unwrap();
        let text = |a: &Solid, b: &Solid, e: BooleanError| match boolean_error(a, b, &e) {
            FeatureError::Kernel(m) => m,
            other => panic!("{other:?}"),
        };
        let hinted = [
            BooleanError::Unsupported(planora_kernel::boolean::TOO_EXPENSIVE.into()),
            BooleanError::Unsupported(
                "Der Schnitt von Freiformfläche mit Zylinder wird nicht unterstützt: x".into(),
            ),
            BooleanError::AmbiguousContact,
            BooleanError::AmbiguousClassification,
            BooleanError::UnsupportedIntersection {
                a: "Freiformfläche",
                b: "Ebene",
            },
        ];
        for e in hinted {
            let m = text(&freeform, &plain, e.clone());
            assert!(m.starts_with("Bei Körpern mit Freiformflächen: "), "{m}");
            assert!(m.contains(&e.to_string()), "{m}");
            assert_eq!(m.matches("; bitte ").count(), 1, "genau ein Vorschlag: {m}");
            // Andere Reihenfolge: auch mit Hinweis; rein analytisch: ohne.
            assert!(text(&plain, &freeform, e.clone()).contains("Freiformflächen"));
            assert_eq!(text(&plain, &plain, e.clone()), e.to_string());
        }
        // Grenzen analytischer Wege und Fehler, die keine Grenzen sind: alter Text, auch mit
        // Freiformkörper.
        let plain_kinds = [
            BooleanError::EmptyResult,
            BooleanError::NotClosed,
            BooleanError::ResultInvalid("x".into()),
            BooleanError::InvalidInput("y".into()),
            BooleanError::UnsupportedIntersection {
                a: "Ebene",
                b: "Kegel",
            },
            BooleanError::Unsupported(
                "Der Schnitt von Ebene mit Torus wird nicht unterstützt".into(),
            ),
        ];
        for e in plain_kinds {
            assert_eq!(text(&freeform, &plain, e.clone()), e.to_string());
        }
    }

    #[test]
    fn huge_numbers_become_an_error_status_instead_of_a_body() {
        use crate::model::FeatureStatus;
        let mut doc = Document::new();
        let a = doc
            .add_feature(
                "A",
                FeatureKind::Box {
                    size: DVec3::splat(1e300),
                    origin: DVec3::new(1e300, 0.0, 0.0),
                },
            )
            .unwrap();
        match &doc.feature(a).unwrap().status {
            FeatureStatus::Error(e) => assert!(e.contains("außerhalb"), "{e}"),
            s => panic!("{s:?}"),
        }
        assert!(doc.visible_bodies().is_empty());
        // Lage allein zu groß, Maß klein
        let b = doc
            .add_feature(
                "B",
                FeatureKind::Sphere {
                    radius: 1.0,
                    origin: DVec3::new(0.0, 2e9, 0.0),
                },
            )
            .unwrap();
        assert!(matches!(
            doc.feature(b).unwrap().status,
            FeatureStatus::Error(_)
        ));
        // Verschieben über die Grenze
        let c = doc
            .add_feature(
                "C",
                FeatureKind::Box {
                    size: DVec3::ONE,
                    origin: DVec3::ZERO,
                },
            )
            .unwrap();
        let body = BodyRef {
            feature: c,
            index: 0,
        };
        let m = doc
            .add_feature(
                "M",
                FeatureKind::Move {
                    body,
                    delta: DVec3::new(MAX_COORD_MM, 0.0, 0.0),
                },
            )
            .unwrap();
        assert!(
            matches!(doc.feature(m).unwrap().status, FeatureStatus::Error(_)),
            "Ecke bei 1e9 + 1"
        );
        doc.update(m, |k| {
            if let FeatureKind::Move { delta, .. } = k {
                *delta = DVec3::new(f64::MAX, 0.0, 0.0);
            }
        })
        .unwrap();
        assert!(matches!(
            doc.feature(m).unwrap().status,
            FeatureStatus::Error(_)
        ));
        doc.update(m, |k| {
            if let FeatureKind::Move { delta, .. } = k {
                *delta = DVec3::new(5.0, 0.0, 0.0);
            }
        })
        .unwrap();
        assert_eq!(doc.feature(m).unwrap().status, FeatureStatus::Ok);
        doc.check_invariants().unwrap();
    }

    #[test]
    fn an_implausible_boolean_volume_is_reported() {
        let doc = Document::new();
        let big = doc.kernel.make_box(DVec3::splat(4.0)).unwrap();
        let tiny = doc.kernel.make_box(DVec3::splat(1.0)).unwrap();
        // Vereinigung kleiner als jeder Eingangskörper: unplausibel
        let r = check_volume(&doc, BoolOp::Union, &big, &big, &tiny);
        assert_eq!(r, Err(FeatureError::ImplausibleResult));
        assert!(
            FeatureError::ImplausibleResult
                .to_string()
                .contains("unplausibel")
        );
        assert_eq!(check_volume(&doc, BoolOp::Union, &big, &tiny, &big), Ok(()));
    }

    #[test]
    fn the_blend_volume_probe_rejects_a_wrong_estimate() {
        use planora_kernel::blend::topo::{edge_uses, face_name};
        let doc = Document::new();
        let s = doc.kernel.make_box(DVec3::splat(10.0)).unwrap();
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let n: Vec<String> = edge_uses(&s, id)
                    .iter()
                    .map(|u| face_name(&s, u.face))
                    .collect();
                n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
            })
            .unwrap();
        let request = BlendRequest {
            edges: vec![BlendEdge {
                edge: e,
                first_face: None,
            }],
            kind: planora_kernel::BlendKind::Fillet { radius: 2.0 },
            tangent_chain: false,
        };
        let mut b = doc.kernel.blend(&s, &request).unwrap();
        assert_eq!(check_blend_volume(&doc, &s, &b), Ok(()));
        b.volume_change = 500.0;
        assert_eq!(
            check_blend_volume(&doc, &s, &b),
            Err(FeatureError::ImplausibleBlend)
        );
    }

    /// Konvexe und konkave Kante zusammen (Review Stufe 2, I1): Abtrag und Zugabe heben sich
    /// auf, die Probe misst ihren Spielraum an der Summe der Beträge. Ein Ergebnis, das um mehr
    /// als `BLEND_REL` mal diese Summe danebenliegt, bleibt unplausibel.
    #[test]
    fn the_blend_volume_probe_uses_the_gross_estimate_for_mixed_edges() {
        use planora_kernel::{Profile, ProfileLoop};
        use planora_math::{DVec2, Frame};
        let doc = Document::new();
        let pts = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ]
        .map(|(x, y)| DVec2::new(x, y));
        let profile = Profile {
            outer: ProfileLoop::polygon(&pts).unwrap(),
            holes: vec![],
        };
        let s = doc.kernel.extrude(&profile, &Frame::WORLD, 5.0).unwrap();
        let vertical = |x: f64, y: f64| {
            s.edges
                .iter()
                .find(|(_, e)| {
                    let (a, b) = (s.vertices[e.start].point, s.vertices[e.end].point);
                    let at = |p: DVec3| (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9;
                    at(a) && at(b)
                })
                .map(|(id, _)| id)
                .unwrap()
        };
        let request =
            |edges: &[planora_kernel::Id<planora_kernel::Edge>], d1: f64, d2: f64| BlendRequest {
                edges: edges
                    .iter()
                    .map(|&edge| BlendEdge {
                        edge,
                        first_face: None,
                    })
                    .collect(),
                kind: planora_kernel::BlendKind::ChamferTwo {
                    distance1: d1,
                    distance2: d2,
                },
                tangent_chain: false,
            };
        let (concave, convex) = (vertical(4.0, 4.0), vertical(10.0, 0.0));
        let mut b = doc
            .kernel
            .blend(&s, &request(&[concave, convex], 0.5, 0.8))
            .unwrap();
        // Je Kante 0,5 · 0,5 · 0,8 · 5 = 1: netto 0, brutto 2.
        assert!(b.volume_change.abs() < 1e-9, "{}", b.volume_change);
        assert!((b.volume_scale - 2.0).abs() < 1e-9, "{}", b.volume_scale);
        assert_eq!(check_blend_volume(&doc, &s, &b), Ok(()));
        // Falscher Körper: nur die konvexe Kante, größer gefast (ΔV = −4).
        b.solid = doc
            .kernel
            .blend(&s, &request(&[convex], 1.0, 1.6))
            .unwrap()
            .solid;
        assert_eq!(
            check_blend_volume(&doc, &s, &b),
            Err(FeatureError::ImplausibleBlend)
        );
    }
}
