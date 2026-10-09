use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SketchError {
    #[error("Ungültige Eingabe: {0}")]
    InvalidInput(String),
    #[error("Das Objekt gibt es in dieser Skizze nicht")]
    UnknownObject,
    #[error("Entartete Geometrie: {0}")]
    Degenerate(String),
    #[error("Falscher Objekttyp: {0}")]
    WrongType(String),
    #[error("Das Profil ist nicht geschlossen")]
    OpenLoop,
    #[error("Das Profil ist nicht eindeutig: {0}")]
    AmbiguousTopology(String),
    #[error("Das Profil schneidet oder berührt sich selbst oder ein anderes Profil")]
    SelfIntersecting,
    #[error("Die Verschachtelung der Profile ist nicht eindeutig")]
    AmbiguousNesting,
    #[error("Dieses Profil enthält {0} und kann noch nicht zu einem Körper werden")]
    UnsupportedForSolid(String),
    #[error("Diese Kante kann noch nicht in die Skizze kopiert werden: {0}")]
    UnsupportedProjection(String),
    #[error("Der Kernel hat das Profil abgelehnt: {0}")]
    Kernel(String),
}
