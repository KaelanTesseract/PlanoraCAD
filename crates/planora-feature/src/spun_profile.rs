//! Drehprofil (Skizzen 11b, Spec 5): Auswahl, Achse und Einstellungen als Herkunft
//! [`crate::CopySource::SpunProfile`]. In Task 7 nur die Typen; die Rechnung kommt in Task 13.

use crate::model::{BodyRef, FeatureId};
use crate::planes::AxisRef;
use crate::surface_project::CurveSource;
use crate::{EdgeRef, VertexRef};
use planora_sketch::CurveId;

/// Was gedreht wird: ganze Körper oder bestimmte Elemente (Kanten, Ecken, Skizzenkurven).
#[derive(Clone, Debug, PartialEq)]
pub enum SpunSelection {
    Bodies(Vec<BodyRef>),
    Elements(Vec<CurveSource>),
}

/// Drehachse: eine Linie der eigenen Skizze oder ein Bezug (Ursprungsachse, gerade Kante,
/// Linie einer früheren Skizze).
#[derive(Clone, Debug, PartialEq)]
pub enum SpunAxis {
    Own(CurveId),
    Ref(AxisRef),
}

/// „Drehprofil“: Auswahl, Achse, Toleranz (mm) für angenäherte Kurven und „Umkehren“ (Ergebnis
/// auf der anderen Seite der Achse).
#[derive(Clone, Debug, PartialEq)]
pub struct SpunSpec {
    pub selection: SpunSelection,
    pub axis: SpunAxis,
    pub tolerance: f64,
    pub flip: bool,
}

fn edge_origins(e: &EdgeRef, out: &mut Vec<FeatureId>) {
    out.push(FeatureId(e.a.origin));
    out.push(FeatureId(e.b.origin));
}

impl SpunSpec {
    /// Feste Verweise: Körper-Features, Kanten und Ecken der Auswahl, die Achsenkante (siehe
    /// [`crate::CopySource::origins`]).
    pub(crate) fn origins(&self, out: &mut Vec<FeatureId>) {
        match &self.selection {
            SpunSelection::Bodies(bodies) => out.extend(bodies.iter().map(|b| b.feature)),
            SpunSelection::Elements(items) => {
                for s in items {
                    match s {
                        CurveSource::Edge(e) | CurveSource::Vertex(VertexRef { edge: e, .. }) => {
                            edge_origins(e, out)
                        }
                        CurveSource::SketchCurve { .. } | CurveSource::ConstructionPoint(_) => {}
                    }
                }
            }
        }
        if let SpunAxis::Ref(AxisRef::Edge(e)) = &self.axis {
            edge_origins(e, out);
        }
    }

    /// Lose Verweise: Quellskizzen der Elemente und der Achse, Konstruktionspunkte (siehe
    /// [`crate::CopySource::loose_origins`]).
    pub(crate) fn loose_origins(&self) -> Vec<FeatureId> {
        let mut out = Vec::new();
        if let SpunSelection::Elements(items) = &self.selection {
            for s in items {
                match s {
                    CurveSource::SketchCurve { sketch, .. } => out.push(*sketch),
                    CurveSource::ConstructionPoint(id) => out.push(*id),
                    CurveSource::Edge(_) | CurveSource::Vertex(_) => {}
                }
            }
        }
        if let SpunAxis::Ref(AxisRef::SketchLine { sketch, .. }) = &self.axis {
            out.push(*sketch);
        }
        out
    }
}
