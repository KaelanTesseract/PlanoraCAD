//! Datenmodell des Feature-Baums: Features, ihre Arten und das Dokument.

use crate::refs::{EdgeRef, FaceRef};
use planora_kernel::{AnalyticKernel, BoolOp, Solid};
use planora_math::{DVec3, Frame};
use planora_sketch::{CurveId, Sketch};

/// Dauerhafte Kennung eines Features; wird nie wiederverwendet. `0` ist reserviert für
/// „keinem Feature zugewiesen“ (siehe [`planora_kernel::FaceTag`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FeatureId(pub u64);

/// Verweis auf den `index`-ten Ergebniskörper eines Features.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyRef {
    pub feature: FeatureId,
    pub index: usize,
}

/// Grundebenen für Skizzen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardPlane {
    XY,
    XZ,
    YZ,
}

impl StandardPlane {
    /// Rechtshändiges Koordinatensystem der Ebene im Ursprung.
    pub fn frame(self) -> Frame {
        let (x, y, z) = match self {
            StandardPlane::XY => (DVec3::X, DVec3::Y, DVec3::Z),
            StandardPlane::XZ => (DVec3::X, DVec3::Z, -DVec3::Y),
            StandardPlane::YZ => (DVec3::Y, DVec3::Z, DVec3::X),
        };
        Frame {
            origin: DVec3::ZERO,
            x,
            y,
            z,
        }
    }
}

/// Ebene einer Skizze: eine Grundebene oder eine ebene Fläche eines vorhergehenden Features.
#[derive(Clone, Debug, PartialEq)]
pub enum SketchPlane {
    Standard(StandardPlane),
    OnFace(FaceRef),
}

/// In die Skizze kopierte Körperkante: die Skizzenkurve und der Verweis auf die Kante.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeCopy {
    pub curve: CurveId,
    pub edge: EdgeRef,
}

#[derive(Clone, Debug)]
pub struct SketchFeature {
    pub sketch: Sketch,
    pub plane: SketchPlane,
    pub copies: Vec<EdgeCopy>,
}

/// Art eines Features mit seinen Parametern (Maße in Millimetern).
#[derive(Clone, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "wenige Features je Dokument; die Skizze direkt zu halten hält die Schnittstelle einfach"
)]
pub enum FeatureKind {
    Sketch(SketchFeature),
    Box {
        size: DVec3,
        origin: DVec3,
    },
    Cylinder {
        radius: f64,
        height: f64,
        origin: DVec3,
    },
    Sphere {
        radius: f64,
        origin: DVec3,
    },
    Cone {
        radius: f64,
        height: f64,
        origin: DVec3,
    },
    Extrude {
        sketch: FeatureId,
        profile: usize,
        height: f64,
    },
    Revolve {
        sketch: FeatureId,
        profile: usize,
        axis: CurveId,
    },
    Boolean {
        target: BodyRef,
        tool: BodyRef,
        op: BoolOp,
    },
    Move {
        body: BodyRef,
        delta: DVec3,
    },
}

impl FeatureKind {
    /// Deutsche Bezeichnung der Feature-Art für die Oberfläche.
    pub fn label(&self) -> &'static str {
        match self {
            FeatureKind::Sketch(_) => "Skizze",
            FeatureKind::Box { .. } => "Quader",
            FeatureKind::Cylinder { .. } => "Zylinder",
            FeatureKind::Sphere { .. } => "Kugel",
            FeatureKind::Cone { .. } => "Kegel",
            FeatureKind::Extrude { .. } => "Extrusion",
            FeatureKind::Revolve { .. } => "Rotation",
            FeatureKind::Boolean { .. } => "Boolesche Operation",
            FeatureKind::Move { .. } => "Verschieben",
        }
    }

    /// Alle Features, auf die dieses verweist; aufsteigend sortiert, ohne Doppelte.
    pub fn dependencies(&self) -> Vec<FeatureId> {
        let mut deps = match self {
            FeatureKind::Sketch(s) => {
                let mut d = Vec::new();
                if let SketchPlane::OnFace(r) = &s.plane {
                    d.push(FeatureId(r.origin));
                }
                for c in &s.copies {
                    d.push(FeatureId(c.edge.a.origin));
                    d.push(FeatureId(c.edge.b.origin));
                }
                d
            }
            FeatureKind::Box { .. }
            | FeatureKind::Cylinder { .. }
            | FeatureKind::Sphere { .. }
            | FeatureKind::Cone { .. } => Vec::new(),
            FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } => {
                vec![*sketch]
            }
            FeatureKind::Boolean { target, tool, .. } => vec![target.feature, tool.feature],
            FeatureKind::Move { body, .. } => vec![body.feature],
        };
        deps.sort();
        deps.dedup();
        deps
    }
}

/// Zustand eines Features nach der letzten Neuberechnung.
#[derive(Clone, Debug, PartialEq)]
pub enum FeatureStatus {
    Ok,
    Error(String),
    Skipped(String),
    Suppressed,
    NotComputed,
}

/// Zwischengespeichertes Ergebnis eines Features.
#[derive(Clone, Debug)]
pub(crate) struct FeatureResult {
    pub(crate) bodies: Vec<Solid>,
    pub(crate) sketch: Option<Sketch>,
    /// Ergebnisstand; ändert sich, wenn sich die Körper ändern.
    pub(crate) result_version: u64,
    /// Schlüssel der Eingaben (eigene Version und Ergebnisstände der Abhängigkeiten).
    pub(crate) key: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct Feature {
    pub id: FeatureId,
    pub name: String,
    pub suppressed: bool,
    pub kind: FeatureKind,
    pub status: FeatureStatus,
    /// Wird bei jeder Änderung der Parameter erhöht.
    pub(crate) version: u64,
    pub(crate) result: Option<FeatureResult>,
    /// Schlüssel der Eingaben und Meldung der zuletzt fehlgeschlagenen Berechnung. Bei gleichen
    /// Eingaben wird der Fehler übernommen statt erneut gerechnet.
    pub(crate) failed: Option<(Vec<u64>, String)>,
}

/// Geordnete Liste der Features mit Rückrollmarker.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) features: Vec<Feature>,
    /// Anzahl der aktiven Features von vorn; Features ab diesem Index sind zurückgestellt.
    pub(crate) marker: usize,
    /// Nächste zu vergebende Feature-Id (beginnt bei 1, da 0 „nicht zugewiesen“ bedeutet).
    pub(crate) next_id: u64,
    /// Zähler für Feature-Versionen und Ergebnisstände (beginnt bei 1; 0 heißt „kein Ergebnis“).
    pub(crate) next_version: u64,
    pub(crate) kernel: AnalyticKernel,
    /// Sehnentoleranz der Tessellierung für die Volumenprobe Boolescher Operationen.
    pub(crate) chord_tolerance: f64,
}

/// Standard-Sehnentoleranz in Millimetern (wie in der App).
pub const DEFAULT_CHORD_TOLERANCE: f64 = 0.05;

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Leeres Dokument mit der Standard-Sehnentoleranz.
    pub fn new() -> Document {
        Document::with_options(DEFAULT_CHORD_TOLERANCE)
    }

    /// Leeres Dokument mit eigener Sehnentoleranz für die Volumenprobe. Ein nicht endlicher
    /// oder nicht positiver Wert wird durch [`DEFAULT_CHORD_TOLERANCE`] ersetzt.
    pub fn with_options(chord_tolerance: f64) -> Document {
        let chord_tolerance = if chord_tolerance.is_finite() && chord_tolerance > 0.0 {
            chord_tolerance
        } else {
            DEFAULT_CHORD_TOLERANCE
        };
        Document {
            features: Vec::new(),
            marker: 0,
            next_id: 1,
            next_version: 1,
            kernel: AnalyticKernel::default(),
            chord_tolerance,
        }
    }

    pub fn kernel(&self) -> &AnalyticKernel {
        &self.kernel
    }

    /// Vergibt die nächste Version (für Parameter und Ergebnisstände gemeinsam, nie 0).
    pub(crate) fn bump(&mut self) -> u64 {
        let v = self.next_version;
        self.next_version += 1;
        v
    }

    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    pub fn feature(&self, id: FeatureId) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }

    pub fn index_of(&self, id: FeatureId) -> Option<usize> {
        self.features.iter().position(|f| f.id == id)
    }

    pub fn marker(&self) -> usize {
        self.marker
    }
}
