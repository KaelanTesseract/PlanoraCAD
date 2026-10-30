//! Fehler der Rundungen und Fasen (Meldungen deutsch, für die Oberfläche bestimmt).

use planora_i18n::tr;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum BlendError {
    InvalidParameter(String),
    NoEdges,
    EdgeNotFound,
    InvalidInput(String),
    SeamEdge,
    NearlyTangent { angle_deg: f64 },
    TooLarge { face: String },
    CurvatureTooSmall { face: String },
    SpineLost,
    Unsupported(String),
    Corner(String),
    TooExpensive,
    ResultInvalid(String),
}

impl fmt::Display for BlendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            BlendError::InvalidParameter(m) => tr!("kernel-blend-invalid-parameter", msg = m),
            BlendError::NoEdges => tr!("kernel-blend-no-edges"),
            BlendError::EdgeNotFound => tr!("kernel-blend-edge-not-found"),
            BlendError::InvalidInput(m) => tr!("kernel-blend-invalid-input", msg = m),
            BlendError::SeamEdge => tr!("kernel-blend-seam-edge"),
            BlendError::NearlyTangent { angle_deg } => tr!(
                "kernel-blend-nearly-tangent",
                angle = format!("{angle_deg:.1}")
            ),
            BlendError::TooLarge { face } => tr!("kernel-blend-too-large", face = face),
            BlendError::CurvatureTooSmall { face } => {
                tr!("kernel-blend-curvature-too-small", face = face)
            }
            BlendError::SpineLost => tr!("kernel-blend-spine-lost"),
            BlendError::Unsupported(m) => m.clone(),
            BlendError::Corner(m) => tr!("kernel-blend-corner", msg = m),
            BlendError::TooExpensive => tr!("kernel-blend-too-expensive"),
            BlendError::ResultInvalid(m) => tr!("kernel-blend-result-invalid", msg = m),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for BlendError {}
