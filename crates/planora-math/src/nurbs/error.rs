//! Fehler der NURBS-Konstruktoren und -Rechnungen (Meldungen auf Deutsch).

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NurbsError {
    #[error("Ungültiger Grad {0} (erlaubt: 1 bis {max})", max = crate::nurbs::MAX_DEGREE)]
    Degree(usize),
    #[error("Der Knotenvektor ist ungültig: {0}")]
    Knots(String),
    #[error("Ungültige Kontrollpunkte: {0}")]
    Control(String),
    #[error("Ungültige Gewichte: {0}")]
    Weights(String),
    #[error("Die Annäherung der Kurve ist nicht gelungen: {0}")]
    Fit(String),
    #[error("Ungültige Eingabe: {0}")]
    Input(String),
}
