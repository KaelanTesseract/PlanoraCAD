//! Fehler des Feature-Baums (Meldungen für die Oberfläche, in der Sprache des Programms).

use planora_i18n::tr;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum FeatureError {
    UnknownFeature,
    ForwardReference,
    FaceNotFound {
        origin: u64,
        role: String,
    },
    EdgeNotFound {
        a: String,
        b: String,
    },
    UntaggedFace,
    UntaggedEdge,
    BodyNotFound {
        feature: u64,
        index: usize,
    },
    Kernel(String),
    Sketch(String),
    ImplausibleResult,
    ImplausibleFreeformResult,
    ImplausibleBlend,
    MeshBodyLocked,
    MeshMissing(u64),
    StepShapeMissing(u64),
    /// Der gespeicherte STEP-Text lässt sich nicht zu einem Körper bauen (P14: nur dieses
    /// Feature bekommt den Fehlerstatus, das Dokument lädt).
    StepShape(String),
    /// Ein Import wurde ganz zurückgenommen; Meldung „„{Name}“: {Grund}“, fertig für die
    /// Oberfläche ([`crate::Document::add_step_bodies`]).
    StepImport(String),
    HasDependents(Vec<u64>),
    InvalidInput(String),
    OutOfRange(String),
    /// Ungültige Formel oder Parametertabelle (Meldung fertig für die Oberfläche).
    Formula(String),
    /// Ein Parameter lässt sich nicht löschen, weil Formeln ihn nennen.
    ParameterInUse {
        name: String,
        users: Vec<String>,
    },
}

impl fmt::Display for FeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            FeatureError::UnknownFeature => tr!("feature-err-unknown-feature"),
            FeatureError::ForwardReference => tr!("feature-err-forward-reference"),
            FeatureError::FaceNotFound { origin, role } => {
                tr!(
                    "feature-err-face-not-found",
                    role = role.as_str(),
                    origin = origin.to_string()
                )
            }
            FeatureError::EdgeNotFound { a, b } => {
                tr!("feature-err-edge-not-found", a = a.as_str(), b = b.as_str())
            }
            FeatureError::UntaggedFace => tr!("feature-err-untagged-face"),
            FeatureError::UntaggedEdge => tr!("feature-err-untagged-edge"),
            FeatureError::BodyNotFound { feature, index } => tr!(
                "feature-err-body-not-found",
                index = index.to_string(),
                feature = feature.to_string()
            ),
            FeatureError::Kernel(d) => tr!("feature-err-kernel", detail = d.as_str()),
            FeatureError::Sketch(d) => tr!("feature-err-sketch", detail = d.as_str()),
            FeatureError::ImplausibleResult => tr!("feature-err-implausible"),
            FeatureError::ImplausibleFreeformResult => tr!("feature-err-implausible-freeform"),
            FeatureError::ImplausibleBlend => tr!("feature-err-implausible-blend"),
            FeatureError::MeshBodyLocked => tr!("feature-mesh-locked"),
            FeatureError::MeshMissing(id) => tr!("feature-err-mesh-missing", id = id.to_string()),
            FeatureError::StepShapeMissing(id) => {
                tr!("feature-err-step-shape-missing", id = id.to_string())
            }
            FeatureError::StepShape(d) => tr!("feature-err-step-shape", detail = d.as_str()),
            FeatureError::StepImport(m) => m.clone(),
            FeatureError::HasDependents(ids) => {
                tr!("feature-err-dependents", ids = join_ids(ids))
            }
            FeatureError::InvalidInput(d) => tr!("feature-err-invalid-input", detail = d.as_str()),
            FeatureError::OutOfRange(what) => tr!(
                "feature-err-out-of-range",
                what = what.as_str(),
                max = planora_i18n::len_arg(
                    crate::compute::MAX_COORD_MM,
                    format!("{:e}", crate::compute::MAX_COORD_MM)
                ),
                unit = planora_i18n::unit_symbol()
            ),
            FeatureError::Formula(m) => m.clone(),
            FeatureError::ParameterInUse { name, users } => tr!(
                "feature-err-parameter-in-use",
                name = name.as_str(),
                users = users.join(", ")
            ),
        };
        f.write_str(&text)
    }
}

fn join_ids(ids: &[u64]) -> String {
    ids.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
