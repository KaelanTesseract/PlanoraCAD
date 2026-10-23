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
    #[error(
        "Bei Körpern mit Freiformflächen: Das Ergebnis ist unplausibel (Volumenprobe fehlgeschlagen); bitte Lage oder Maße eines Körpers leicht ändern"
    )]
    ImplausibleFreeformResult,
    #[error("Das Ergebnis der Rundung oder Fase ist unplausibel (Volumenprobe fehlgeschlagen)")]
    ImplausibleBlend,
    #[error("Netz-Körper lassen sich nicht bearbeiten")]
    MeshBodyLocked,
    #[error("Das Netz {0} fehlt im Dokument")]
    MeshMissing(u64),
    #[error("Die STEP-Form {0} fehlt im Dokument")]
    StepShapeMissing(u64),
    /// Der gespeicherte STEP-Text lässt sich nicht zu einem Körper bauen (P14: nur dieses
    /// Feature bekommt den Fehlerstatus, das Dokument lädt).
    #[error("Die gespeicherte STEP-Form lässt sich nicht lesen: {0}")]
    StepShape(String),
    /// Ein Import wurde ganz zurückgenommen; Meldung „„{Name}“: {Grund}“, fertig für die
    /// Oberfläche ([`crate::Document::add_step_bodies`]).
    #[error("{0}")]
    StepImport(String),
    #[error("Folgende Features hängen davon ab: {}", join_ids(.0))]
    HasDependents(Vec<u64>),
    #[error("Ungültige Eingabe: {0}")]
    InvalidInput(String),
    #[error(
        "{0} liegt außerhalb des darstellbaren Bereichs (Beträge höchstens {max:e} mm)",
        max = crate::compute::MAX_COORD_MM
    )]
    OutOfRange(String),
    /// Ungültige Formel oder Parametertabelle (Meldung fertig für die Oberfläche).
    #[error("{0}")]
    Formula(String),
    /// Ein Parameter lässt sich nicht löschen, weil Formeln ihn nennen.
    #[error("{name} wird in {} benutzt.", .users.join(", "))]
    ParameterInUse { name: String, users: Vec<String> },
}

fn join_ids(ids: &[u64]) -> String {
    ids.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
