//! Datentypen für Maße (Teilprojekt 12): Verweise auf Modellgeometrie, Maßarten, Toleranzen,
//! Maßketten und die Fehler einer Maßangabe. Nur Typen; Messung und Darstellung folgen später.

use crate::model::ViewId;
use planora_math::DVec3;

/// Kennung eines Maßes innerhalb einer Zeichnung (nie wiederverwendet).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DimId(pub u64);

/// Art eines Maßes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimKind {
    Horizontal,
    Vertical,
    Aligned,
    Diameter,
    Radius,
    Angle,
}

/// Anfang oder Ende einer Kante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndSel {
    Start,
    End,
}

/// Marke einer Fläche: Herkunft, Rolle und Hinweispunkt (wie im Modell, aber ohne
/// Abhängigkeit von `planora-feature`).
#[derive(Clone, Debug, PartialEq)]
pub struct TagData {
    pub origin: u64,
    pub role: String,
    pub hint: DVec3,
}

/// Verweis auf Modellgeometrie ohne Abhängigkeit von `planora-feature`.
#[derive(Clone, Debug, PartialEq)]
pub enum GeoRefData {
    /// Fläche (Zylinder, Kugel, Ebene …).
    Face(TagData),
    /// Kante über die zwei angrenzenden Flächen.
    Edge { a: TagData, b: TagData, hint: DVec3 },
}

/// Ein Bezug eines Maßes.
#[derive(Clone, Debug, PartialEq)]
pub enum GeoRef {
    Edge(GeoRefData),
    EdgeEnd { edge: GeoRefData, end: EndSel },
    Center(GeoRefData),
    Face(GeoRefData),
}

/// Toleranzangabe eines Maßes.
#[derive(Clone, Debug, PartialEq)]
pub enum Tolerance {
    None,
    Symmetric(f64),
    Limits { upper: f64, lower: f64 },
    LimitSizes { upper: f64, lower: f64 },
    Fit(String),
}

/// Maßkette (gemeinsame Maßlinie) oder Bezugsmaße (gestapelte Maßlinien).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainMode {
    Chain,
    Baseline,
}

/// Zugehörigkeit eines Maßes zu einer Gruppe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainSpec {
    pub group: u64,
    pub mode: ChainMode,
    pub order: u32,
}

/// Ein Maß einer Zeichnung.
#[derive(Clone, Debug, PartialEq)]
pub struct DimSpec {
    pub id: DimId,
    pub view: ViewId,
    pub kind: DimKind,
    pub refs: Vec<GeoRef>,
    /// Abstand der Maßlinie in Blatt-mm.
    pub offset: f64,
    /// Winkel der Führungslinie in Grad (nur Ø/R).
    pub angle: f64,
    /// Verschiebung des Textes in Blatt-mm.
    pub text_shift: (f64, f64),
    pub tolerance: Tolerance,
    pub decimals: u8,
    pub text_override: Option<String>,
    pub chain: Option<ChainSpec>,
    pub suggested: bool,
}

/// Aufgelöste Modellgeometrie eines Bezugs.
#[derive(Clone, Debug, PartialEq)]
pub enum Geo3 {
    Point(DVec3),
    Line {
        a: DVec3,
        b: DVec3,
    },
    Circle {
        center: DVec3,
        normal: DVec3,
        radius: f64,
    },
    Axis {
        origin: DVec3,
        dir: DVec3,
        radius: f64,
    },
    Sphere {
        center: DVec3,
        radius: f64,
    },
}

/// Fehler einer Maßangabe.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum DimError {
    #[error("Die Kante steht nicht quer zum Maß.")]
    NotAcross,
    #[error("Der Bezug ist in dieser Ansicht nicht zu sehen.")]
    NotVisible,
    #[error("Der Bezug ist nicht eindeutig.")]
    Ambiguous,
    #[error("Der Bezug wurde nicht gefunden: {0}")]
    Lost(String),
    #[error("In der Isometrie gibt es keine Maße.")]
    NotAllowedInIsometric,
    #[error("Es gibt zu viele Maße in dieser Zeichnung.")]
    TooMany,
    #[error("Das Maß ist ungültig: {0}")]
    Invalid(String),
}

/// Mindest- und Höchstzahl der Bezüge einer Maßart (bei Ketten je Kettenglied).
pub fn ref_count(kind: DimKind) -> (usize, usize) {
    match kind {
        DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned | DimKind::Angle => (2, 2),
        DimKind::Diameter | DimKind::Radius => (1, 1),
    }
}

impl DimKind {
    /// Siehe [`ref_count`].
    pub fn ref_count(self) -> (usize, usize) {
        ref_count(self)
    }
}
