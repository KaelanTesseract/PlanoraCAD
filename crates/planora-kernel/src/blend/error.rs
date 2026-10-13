//! Fehler der Rundungen und Fasen (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum BlendError {
    #[error("Ungültiges Maß: {0}")]
    InvalidParameter(String),
    #[error("Es ist keine Kante gewählt")]
    NoEdges,
    #[error("Eine gewählte Kante gehört nicht zu diesem Körper")]
    EdgeNotFound,
    #[error("Der Körper ist ungültig: {0}")]
    InvalidInput(String),
    #[error("Die Kante ist die Naht einer gekrümmten Fläche und lässt sich nicht runden")]
    SeamEdge,
    #[error(
        "Die Flächen an der Kante gehen fast tangential ineinander über (Kantenwinkel {angle_deg:.1}°); dort ist keine Rundung oder Fase möglich"
    )]
    NearlyTangent { angle_deg: f64 },
    #[error("Das Maß ist zu groß für die Fläche „{face}“; bitte ein kleineres Maß wählen")]
    TooLarge { face: String },
    #[error(
        "Die Fläche „{face}“ ist enger gekrümmt als der Radius; bitte einen kleineren Radius wählen"
    )]
    CurvatureTooSmall { face: String },
    #[error("Die Kugel findet entlang der Kante keine Lage (die Bahn bricht ab)")]
    SpineLost,
    #[error("{0}")]
    Unsupported(String),
    #[error("Diese Ecke lässt sich nicht runden: {0}")]
    Corner(String),
    #[error(
        "Die Berechnung der Rundung ist zu aufwendig (Arbeitsgrenze erreicht); bitte weniger Kanten oder eine einfachere Form verwenden"
    )]
    TooExpensive,
    #[error("Das Ergebnis der Rundung ist ungültig: {0}")]
    ResultInvalid(String),
}
