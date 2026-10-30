use planora_i18n::tr;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SketchError {
    InvalidInput(String),
    UnknownObject,
    Degenerate(String),
    WrongType(String),
    OpenLoop,
    AmbiguousTopology(String),
    SelfIntersecting,
    AmbiguousNesting,
    UnsupportedForSolid(String),
    UnsupportedProjection(String),
    Kernel(String),
}

impl fmt::Display for SketchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            SketchError::InvalidInput(d) => tr!("sketch-err-invalid-input", detail = d.as_str()),
            SketchError::UnknownObject => tr!("sketch-err-unknown-object"),
            SketchError::Degenerate(d) => tr!("sketch-err-degenerate", detail = d.as_str()),
            SketchError::WrongType(d) => tr!("sketch-err-wrong-type", detail = d.as_str()),
            SketchError::OpenLoop => tr!("sketch-err-open-loop"),
            SketchError::AmbiguousTopology(d) => {
                tr!("sketch-err-ambiguous-topology", detail = d.as_str())
            }
            SketchError::SelfIntersecting => tr!("sketch-err-self-intersecting"),
            SketchError::AmbiguousNesting => tr!("sketch-err-ambiguous-nesting"),
            SketchError::UnsupportedForSolid(w) => {
                tr!("sketch-err-unsupported-solid", what = w.as_str())
            }
            SketchError::UnsupportedProjection(d) => {
                tr!("sketch-err-unsupported-projection", detail = d.as_str())
            }
            SketchError::Kernel(d) => tr!("sketch-err-kernel", detail = d.as_str()),
        };
        f.write_str(&text)
    }
}
