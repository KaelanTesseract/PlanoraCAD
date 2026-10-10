//! Fehler des Feature-Baums (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum FeatureError {
    #[error("Das Feature gibt es in diesem Dokument nicht")]
    UnknownFeature,
    #[error("Ein Feature darf nur auf Features verweisen, die vor ihm stehen")]
    ForwardReference,
    #[error("Fläche „{role}“ von Feature {origin} ist nicht mehr vorhanden")]
    FaceNotFound { origin: u64, role: String },
    #[error("Die Kante zwischen den Flächen „{a}“ und „{b}“ ist nicht mehr vorhanden")]
    EdgeNotFound { a: String, b: String },
    #[error("Die Fläche ist keinem Feature zugeordnet und kann nicht verwendet werden")]
    UntaggedFace,
    #[error("Die Kante ist keinem Feature zugeordnet und kann nicht verwendet werden")]
    UntaggedEdge,
    #[error("Körper {index} von Feature {feature} ist nicht vorhanden")]
    BodyNotFound { feature: u64, index: usize },
    #[error("Der Kernel hat die Operation abgelehnt: {0}")]
    Kernel(String),
    #[error("Fehler in der Skizze: {0}")]
    Sketch(String),
    #[error("Das Ergebnis ist unplausibel (Volumenprobe fehlgeschlagen)")]
    ImplausibleResult,
    #[error("Folgende Features hängen davon ab: {}", join_ids(.0))]
    HasDependents(Vec<u64>),
    #[error("Ungültige Eingabe: {0}")]
    InvalidInput(String),
}

fn join_ids(ids: &[u64]) -> String {
    ids.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
