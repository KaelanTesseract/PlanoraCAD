//! Datenmodell des Feature-Baums: Features, ihre Arten und das Dokument.

use crate::refs::{EdgeRef, FaceRef};
use crate::step_bodies::{StepShape, StepShapeId};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_kernel::{AnalyticKernel, BlendKind, BoolOp, Solid};
use planora_math::{DQuat, DVec3, Frame};
use planora_sketch::{ConstraintId, CurveId, Sketch};
use std::collections::BTreeMap;
use std::sync::Arc;

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

/// Kennung eines Netzes in der Netztabelle des Dokuments (ab 1); wird nie wiederverwendet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(pub u64);

/// Kleinster und größter Maßstab eines Netz-Körpers (Spec §6).
pub const MIN_MESH_SCALE: f64 = 1e-6;
pub const MAX_MESH_SCALE: f64 = 1e6;
/// Kleinster und größter Fasenwinkel in Grad, beide eingeschlossen (wie im Fenster „Fase“ und
/// im Kernel, Teilprojekt 8 P4).
pub const MIN_CHAMFER_ANGLE_DEG: f64 = 1.0;
pub const MAX_CHAMFER_ANGLE_DEG: f64 = 179.0;
/// Größte Abweichung der Länge der Drehung von 1 (Spec §6 „normiert“, Plan P17); deckt das
/// Runden beim Speichern und beim Umrechnen aus Grad ab.
pub const ROTATION_NORM_TOL: f64 = 1e-6;
/// Meldung der Bearbeitungssperre (Spec §6), gleich dem Text von
/// [`crate::FeatureError::MeshBodyLocked`].
pub const MESH_LOCKED: &str = "Netz-Körper lassen sich nicht bearbeiten";

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

/// Formeln der Skizzenmaße: Abhängigkeit → Formeltext (Nebentabelle, Spec §4a.1). Der Wert in
/// der Abhängigkeit ist der zuletzt berechnete; Winkelformeln ergeben Grad, gespeichert wird
/// Bogenmaß.
pub type SketchFormulas = BTreeMap<ConstraintId, String>;

#[derive(Clone, Debug)]
pub struct SketchFeature {
    pub sketch: Sketch,
    pub plane: SketchPlane,
    pub copies: Vec<EdgeCopy>,
    /// Formeln der bestimmenden Maße (nur lebende, nicht anzeigende Maß-Abhängigkeiten, siehe
    /// [`SketchFeature::prune_formulas`]).
    pub formulas: SketchFormulas,
}

impl SketchFeature {
    /// Entfernt Formeln an fehlenden, nicht maßgebenden (`!is_dimension()`) oder anzeigenden
    /// (`driven`) Abhängigkeiten; `true`, wenn sich etwas änderte.
    pub fn prune_formulas(&mut self) -> bool {
        let before = self.formulas.len();
        let sketch = &self.sketch;
        self.formulas.retain(|id, _| {
            sketch
                .constraint(*id)
                .is_some_and(|c| c.kind.is_dimension() && !c.driven)
        });
        self.formulas.len() != before
    }
}

/// Gleich heißt: gleicher Stand der Skizze ([`Sketch::revision`]), gleicher Skizzenname (ändert
/// den Stand nicht, wird aber gespeichert), gleiche Ebene, gleiche Kantenkopien, gleiche
/// Formeltexte (eine Formel ändert den Stand der Skizze nicht). Zwei getrennt gebaute Skizzen
/// mit gleichem Inhalt sind ungleich (vorsichtig: höchstens ein überflüssiger Schritt im Verlauf
/// oder eine Neuberechnung mehr).
impl PartialEq for SketchFeature {
    fn eq(&self, other: &Self) -> bool {
        self.sketch.revision() == other.sketch.revision()
            && self.sketch.name == other.sketch.name
            && self.plane == other.plane
            && self.copies == other.copies
            && self.formulas == other.formulas
    }
}

/// Art einer Verrundung (Maße in mm, Winkel in Grad, Positionen in Prozent der Kettenlänge).
#[derive(Clone, Debug, PartialEq)]
pub enum BlendArt {
    Rundung {
        radius: f64,
    },
    /// Punkte (Position in % der Kettenlänge, Radius), Positionen streng steigend.
    RundungVariabel {
        punkte: Vec<(f64, f64)>,
    },
    FaseGleich {
        abstand: f64,
    },
    /// Abstand auf Fläche 1 (Marke `EdgeRef.a`) und auf Fläche 2.
    FaseZwei {
        abstand1: f64,
        abstand2: f64,
    },
    /// Abstand auf Fläche 1 und Winkel der Fase zu Fläche 1.
    FaseWinkel {
        abstand: f64,
        winkel: f64,
    },
}

impl BlendArt {
    pub fn is_chamfer(&self) -> bool {
        matches!(
            self,
            BlendArt::FaseGleich { .. } | BlendArt::FaseZwei { .. } | BlendArt::FaseWinkel { .. }
        )
    }

    /// Bezeichnung der Feature-Art: „Rundung“ oder „Fase“.
    pub fn label(&self) -> &'static str {
        if self.is_chamfer() { "Fase" } else { "Rundung" }
    }

    /// Ausführliche Bezeichnung für Eigenschaften und Fenster.
    pub fn kind_label(&self) -> &'static str {
        match self {
            BlendArt::Rundung { .. } => "Rundung",
            BlendArt::RundungVariabel { .. } => "Rundung mit variablem Radius",
            BlendArt::FaseGleich { .. } => "Fase mit gleichem Abstand",
            BlendArt::FaseZwei { .. } => "Fase mit zwei Abständen",
            BlendArt::FaseWinkel { .. } => "Fase mit Abstand und Winkel",
        }
    }

    /// Alle Zahlen (für die Bereichsprüfung).
    pub fn numbers(&self) -> Vec<f64> {
        match self {
            BlendArt::Rundung { radius } => vec![*radius],
            BlendArt::RundungVariabel { punkte } => {
                punkte.iter().flat_map(|(p, r)| [*p, *r]).collect()
            }
            BlendArt::FaseGleich { abstand } => vec![*abstand],
            BlendArt::FaseZwei { abstand1, abstand2 } => vec![*abstand1, *abstand2],
            BlendArt::FaseWinkel { abstand, winkel } => vec![*abstand, *winkel],
        }
    }

    /// Anfrage-Art des Kernels (Grad → Bogenmaß, Prozent → Anteil).
    pub fn kernel_kind(&self) -> BlendKind {
        match self {
            BlendArt::Rundung { radius } => BlendKind::Fillet { radius: *radius },
            BlendArt::RundungVariabel { punkte } => BlendKind::VariableFillet {
                points: punkte.iter().map(|(p, r)| (p / 100.0, *r)).collect(),
            },
            BlendArt::FaseGleich { abstand } => BlendKind::ChamferEqual { distance: *abstand },
            BlendArt::FaseZwei { abstand1, abstand2 } => BlendKind::ChamferTwo {
                distance1: *abstand1,
                distance2: *abstand2,
            },
            BlendArt::FaseWinkel { abstand, winkel } => BlendKind::ChamferAngle {
                distance: *abstand,
                angle: winkel.to_radians(),
            },
        }
    }
}

/// Art eines Features mit seinen Parametern (Maße in Millimetern).
#[derive(Clone, Debug, PartialEq)]
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
    /// Rundung oder Fase an Kanten eines Körpers; ersetzt den Körper (wie Verschieben).
    Verrunden {
        body: BodyRef,
        edges: Vec<EdgeRef>,
        art: BlendArt,
        tangentenkette: bool,
    },
    /// Eingelesenes Netz („Netz-Körper“, Spec §6, Plan P10): Verweis in die Netztabelle des
    /// Dokuments und Lage (erst skalieren, dann drehen, dann verschieben). Erzeugt keinen
    /// Kernel-Körper.
    MeshBody {
        mesh: MeshId,
        offset: DVec3,
        rotation: DQuat,
        scale: f64,
    },
    /// Eingelesener exakter Körper („STEP-Körper“, Plan P6): Verweis in die Formtabelle des
    /// Dokuments; keine Zahlenfelder, keine Lage (Verschieben über das Verschieben-Feature).
    StepBody {
        shape: StepShapeId,
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
            FeatureKind::Verrunden { art, .. } => art.label(),
            FeatureKind::MeshBody { .. } => "Netz-Körper",
            FeatureKind::StepBody { .. } => "STEP-Körper",
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
            | FeatureKind::Cone { .. }
            | FeatureKind::MeshBody { .. }
            | FeatureKind::StepBody { .. } => Vec::new(),
            FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } => {
                vec![*sketch]
            }
            FeatureKind::Boolean { target, tool, .. } => vec![target.feature, tool.feature],
            FeatureKind::Move { body, .. } => vec![body.feature],
            FeatureKind::Verrunden { body, edges, .. } => {
                let mut d = vec![body.feature];
                for e in edges {
                    d.push(FeatureId(e.a.origin));
                    d.push(FeatureId(e.b.origin));
                }
                d
            }
        };
        deps.sort();
        deps.dedup();
        deps
    }

    /// Lage eines Netz-Körpers; `None` für alle anderen Arten.
    pub fn mesh_transform(&self) -> Option<MeshTransform> {
        match self {
            FeatureKind::MeshBody {
                offset,
                rotation,
                scale,
                ..
            } => Some(MeshTransform {
                offset: *offset,
                rotation: *rotation,
                scale: *scale,
            }),
            _ => None,
        }
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
    /// Wird bei jeder Änderung der Parameter erhöht (auch wenn eine Formel einen neuen Wert
    /// ergibt).
    pub(crate) version: u64,
    pub(crate) result: Option<FeatureResult>,
    /// Schlüssel der Eingaben und Meldung der zuletzt fehlgeschlagenen Berechnung. Bei gleichen
    /// Eingaben wird der Fehler übernommen statt erneut gerechnet.
    pub(crate) failed: Option<(Vec<u64>, String)>,
    /// Formeln der Zahlenfelder (Nebentabelle, Spec §4a.1); die Felder in `kind` tragen den
    /// zuletzt berechneten Wert.
    pub(crate) formulas: crate::formulas::Formulas,
    /// Erster Formelfehler nach dem letzten Auswertungsdurchgang; ergibt den Status `Error`.
    pub(crate) issue: Option<crate::formulas::FormulaIssue>,
}

impl Feature {
    /// Die Formeln der Zahlenfelder (Feld → Formeltext).
    pub fn formulas(&self) -> &crate::formulas::Formulas {
        &self.formulas
    }

    /// Stand der Parameter (und der Unterdrückung); ändert sich bei jeder solchen Änderung.
    /// Dialoge erkennen daran, dass ihre Kopie veraltet ist.
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// Geordnete Liste der Features mit Rückrollmarker und Rückrollstellen je Körper.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) features: Vec<Feature>,
    /// Anzahl der aktiven Features von vorn; Features ab diesem Index sind zurückgestellt.
    pub(crate) marker: usize,
    /// Rückrollstellen je Körper in Listenordnung (siehe [`Document::set_rollback`]): Eine Stelle
    /// setzt ihren Schritt und alle davon abhängigen zurück. Keine Stelle liegt in der Kette
    /// einer anderen.
    pub(crate) rollbacks: Vec<FeatureId>,
    /// Nächste zu vergebende Feature-Id (beginnt bei 1, da 0 „nicht zugewiesen“ bedeutet).
    pub(crate) next_id: u64,
    /// Zähler für Feature-Versionen und Ergebnisstände (beginnt bei 1; 0 heißt „kein Ergebnis“).
    pub(crate) next_version: u64,
    pub(crate) kernel: AnalyticKernel,
    /// Sehnentoleranz der Tessellierung für die Volumenprobe Boolescher Operationen.
    pub(crate) chord_tolerance: f64,
    /// Zähler der öffentlichen Änderungen (siehe [`Document::edit_count`]).
    pub(crate) edits: u64,
    /// Netze der Netz-Körper (Plan P11); mehrere Features können dasselbe Netz benutzen.
    pub(crate) meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>,
    /// Nächste zu vergebende Netz-Kennung (beginnt bei 1).
    pub(crate) next_mesh: u64,
    /// Formen der STEP-Körper (normalisierter Text, einmal gebauter Körper).
    pub(crate) step_shapes: BTreeMap<StepShapeId, Arc<StepShape>>,
    /// Nächste zu vergebende Form-Kennung (beginnt bei 1).
    pub(crate) next_step_shape: u64,
    /// Zeichnungen des Dokuments (Teilprojekt 11).
    pub(crate) drawings: Vec<planora_drawing::Drawing>,
    /// Nächste zu vergebende Zeichnungskennung (beginnt bei 1).
    pub(crate) next_drawing: u64,
    /// Zähler der Änderungen an Zeichnungen (zählt zu [`Document::edit_count`], nicht zu
    /// [`Document::model_edit_count`]).
    pub(crate) drawing_edits: u64,
    /// Projektangaben für das Schriftfeld (Teilprojekt 3c-1); `None`, bis eine Zeichnung mit
    /// Datum und Benutzer angelegt wurde.
    pub(crate) project_info: Option<planora_drawing::sheet_info::ProjectInfo>,
    /// Eigene Texte der Stücklistenzeilen, nach Körperkennung (Teilprojekt 3c-2); zählt wie die
    /// Zeichnungen zu [`Document::edit_count`], nicht zu [`Document::model_edit_count`].
    pub(crate) parts_overrides: BTreeMap<planora_drawing::BodyKey, planora_drawing::RowOverride>,
    /// Parametertabelle (Tabellenreihenfolge); zählt zum Modell.
    pub(crate) parameters: Vec<crate::params::Parameter>,
    /// Berechnete Werte, je Zeile von `parameters` einer.
    pub(crate) param_values: Vec<f64>,
    /// Nächste zu vergebende Parameterkennung (beginnt bei 1).
    pub(crate) next_param: u64,
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
            rollbacks: Vec::new(),
            next_id: 1,
            next_version: 1,
            kernel: AnalyticKernel::default(),
            chord_tolerance,
            edits: 0,
            meshes: BTreeMap::new(),
            next_mesh: 1,
            step_shapes: BTreeMap::new(),
            next_step_shape: 1,
            drawings: Vec::new(),
            next_drawing: 1,
            drawing_edits: 0,
            project_info: None,
            parts_overrides: BTreeMap::new(),
            parameters: Vec::new(),
            param_values: Vec::new(),
            next_param: 1,
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

    /// Zähler aller öffentlichen Änderungen am Dokument (Hinzufügen, Ändern, Umbenennen,
    /// Löschen, Unterdrücken, Marker). Die Oberfläche erkennt daran ungespeicherte Änderungen.
    /// Änderungen an Zeichnungen zählen ebenfalls mit.
    pub fn edit_count(&self) -> u64 {
        self.edits + self.drawing_edits
    }

    /// Zähler der Änderungen **an Körpern** (Features); ändert sich bei Zeichnungsänderungen nicht
    /// (Schlüssel der Ansichtsberechnung).
    pub fn model_edit_count(&self) -> u64 {
        self.edits
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
