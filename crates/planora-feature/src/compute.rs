//! Berechnung eines einzelnen Features aus seinen Parametern und den sichtbaren Körpern.

use crate::error::FeatureError;
use crate::model::{BodyRef, Document, EdgeCopy, FeatureKind, SketchFeature, SketchPlane};
use crate::refs::{FaceRef, resolve_edge, resolve_face};
use planora_kernel::{BoolOp, Kernel, Solid};
use planora_math::{DMat4, DVec3};
use planora_mesh::{TessellationOptions, boolean_volume_plausible, mass_properties, tessellate};
use planora_sketch::{Sketch, extrude_profile, plane_of_face, revolve_profile};

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
                .map_err(|e| FeatureError::Kernel(e.to_string()))?;
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

fn nth_profile(sk: &Sketch, n: usize) -> Result<planora_kernel::Profile, FeatureError> {
    sk.profiles()
        .profiles
        .into_iter()
        .nth(n)
        .ok_or_else(|| FeatureError::InvalidInput(format!("Profil {n} gibt es nicht")))
}

/// Der Körper `r`, sofern er vor dem Feature sichtbar ist.
fn visible_body<'a>(
    doc: &'a Document,
    visible: &[BodyRef],
    r: BodyRef,
) -> Result<&'a Solid, FeatureError> {
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
