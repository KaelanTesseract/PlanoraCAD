use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum KernelError {
    #[error("Das Profil ist nicht geschlossen")]
    ProfileNotClosed,
    #[error("Ein Profilsegment hat die Länge null")]
    DegenerateSegment,
    #[error("Das Profil hat die Fläche null")]
    ZeroAreaProfile,
    #[error("Das Maß muss eine Zahl größer als die Toleranz sein (war {0})")]
    InvalidDimension(f64),
    #[error("Das Rotationsprofil muss auf der Achse beginnen und enden")]
    ProfileOffAxis,
    #[error("Das Rotationsprofil darf die Achse nur am Anfang und am Ende berühren")]
    ProfileCrossesAxis,
    #[error("Dieses Rotationssegment wird noch nicht unterstützt: {0}")]
    UnsupportedSegment(&'static str),
    #[error(
        "Die Matrix ist keine Ähnlichkeitstransformation (verschieben, drehen, spiegeln, gleichmäßig skalieren)"
    )]
    NonSimilarityTransform,
    #[error("Das Koordinatensystem ist ungültig (nicht endlich oder nicht orthonormal)")]
    InvalidFrame,
    #[error("Ein Loch liegt nicht vollständig innerhalb der äußeren Schleife")]
    HoleOutsideOuter,
    #[error("Zwei Schleifen des Profils schneiden oder berühren sich")]
    ProfileLoopsIntersect,
}
