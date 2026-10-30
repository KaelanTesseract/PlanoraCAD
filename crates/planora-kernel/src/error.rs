use planora_i18n::tr;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum KernelError {
    ProfileNotClosed,
    DegenerateSegment,
    ZeroAreaProfile,
    InvalidDimension(f64),
    ProfileOffAxis,
    ProfileCrossesAxis,
    UnsupportedSegment(String),
    NonSimilarityTransform,
    InvalidFrame,
    HoleOutsideOuter,
    ProfileLoopsIntersect,
    NestedHoles,
    ProfileSelfIntersects,
    InvalidSurface(String),
    InvalidCurve(String),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            KernelError::ProfileNotClosed => tr!("kernel-profile-not-closed"),
            KernelError::DegenerateSegment => tr!("kernel-degenerate-segment"),
            KernelError::ZeroAreaProfile => tr!("kernel-zero-area-profile"),
            KernelError::InvalidDimension(x) => {
                tr!("kernel-invalid-dimension", x = x.to_string())
            }
            KernelError::ProfileOffAxis => tr!("kernel-profile-off-axis"),
            KernelError::ProfileCrossesAxis => tr!("kernel-profile-crosses-axis"),
            KernelError::UnsupportedSegment(what) => {
                tr!("kernel-unsupported-segment", what = what)
            }
            KernelError::NonSimilarityTransform => tr!("kernel-non-similarity"),
            KernelError::InvalidFrame => tr!("kernel-invalid-frame"),
            KernelError::HoleOutsideOuter => tr!("kernel-hole-outside-outer"),
            KernelError::ProfileLoopsIntersect => tr!("kernel-profile-loops-intersect"),
            KernelError::NestedHoles => tr!("kernel-nested-holes"),
            KernelError::ProfileSelfIntersects => tr!("kernel-profile-self-intersects"),
            KernelError::InvalidSurface(m) => tr!("kernel-invalid-surface", msg = m),
            KernelError::InvalidCurve(m) => tr!("kernel-invalid-curve", msg = m),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for KernelError {}
