//! Datenmodell des Feature-Baums: Features, ihre Arten und das Dokument.

use crate::refs::{EdgeRef, FaceRef};
use crate::step_bodies::{StepShape, StepShapeId};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_i18n::tr;
use planora_kernel::{AnalyticKernel, BlendKind, BoolOp, Solid};
use planora_math::{DQuat, DVec3, Frame};
use planora_sketch::{ConstraintId, CurveId, PointId, ProfileRef, Sketch};
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
pub fn mesh_locked() -> String {
    tr!("feature-mesh-locked")
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

/// Ebene einer Skizze: eine Grundebene, eine ebene Fläche eines vorhergehenden Features oder
/// eine vorhergehende Konstruktionsebene (Skizzen 6).
#[derive(Clone, Debug, PartialEq)]
pub enum SketchPlane {
    Standard(StandardPlane),
    OnFace(FaceRef),
    OnPlane(FeatureId),
}

/// Herkunft einer projizierten oder einbezogenen Skizzengeometrie: eine Körperkante, das
/// `index`-te Stück des Schnitts der Skizzenebene mit einer Fläche oder das `index`-te Stück des
/// Umrisses einer Fläche (je in der Reihenfolge des Kernels, siehe
/// [`planora_kernel::section_by_plane`] und [`planora_kernel::silhouette`]); dazu ab Skizzen 10
/// (Format 20) eingezogene 3D-Geometrie, eine Kurve einer früheren Skizze als Quelle von
/// Projizieren (Kurve als Ziel) oder Schneiden (Punkte als Ziel), die Projektion auf Flächen und
/// die Schnittkurve. Quellen mit mehreren Stücken (Flächenrand, Ebene, Schnittpunkte) werden als
/// Gruppe gespeichert: Alle Kopien der Gruppe tragen dieselbe Herkunft und je einen Stückindex
/// ([`EdgeCopy::piece`], Spec 10).
#[derive(Clone, Debug, PartialEq)]
pub enum CopySource {
    Edge(EdgeRef),
    Section {
        face: FaceRef,
        index: usize,
    },
    Silhouette {
        face: FaceRef,
        index: usize,
    },
    /// 3D-Geometrie einbeziehen (Skizzen 10, Spec 5).
    Include(IncludeRef),
    /// Kurve einer früheren Skizze: projiziert (Ziel ist eine Kurve, ein Stück) oder geschnitten
    /// mit der Skizzenebene (Ziele sind Punkte, ein Stück je Schnittpunkt).
    SketchCurve {
        sketch: FeatureId,
        curve: CurveId,
    },
    /// Auf Fläche projizieren (Skizzen 10, Spec 6; Rechnung in Task 17).
    OnSurface(SurfaceProjection),
    /// Schnittkurve (Skizzen 10, Spec 7; Rechnung in Task 18).
    Intersection(IntersectionSpec),
}

/// Ecke eines Körpers: Anfang (`end == false`) oder Ende einer Kante (wie
/// [`crate::planes::PointRef::EdgeEnd`]).
#[derive(Clone, Debug, PartialEq)]
pub struct VertexRef {
    pub edge: EdgeRef,
    pub end: bool,
}

/// Ebene als Quelle: eine Grundebene oder eine Konstruktionsebene (Feature).
#[derive(Clone, Debug, PartialEq)]
pub enum PlaneId {
    Standard(StandardPlane),
    Feature(FeatureId),
}

/// Achse als Quelle. Konstruktionsachsen als eigene Features gibt es noch nicht; wählbar sind die
/// Achsen des Ursprungs.
pub type AxisId = crate::planes::StdAxis;

/// Konstruktionspunkt als Quelle: das Feature des Punkts. Konstruktionspunkte als eigene
/// Features gibt es noch nicht; ein solcher Verweis ist darum immer ein verlorener Bezug
/// (Platzhalter, damit Format 20 die Art schon kennt).
pub type PointRefId = FeatureId;

/// Was „3D-Geometrie einbeziehen“ einzieht (Skizzen 10, Spec 5).
#[derive(Clone, Debug, PartialEq)]
pub enum IncludeRef {
    /// Eine Körperkante (ein Stück).
    Edge(EdgeRef),
    /// Alle Randkanten einer Fläche, äußere Schleife zuerst, je Kante ein Stück.
    FaceBoundary(FaceRef),
    /// Eine Körperecke (ein Punkt).
    Vertex(VertexRef),
    /// Die vier Randlinien des Rechtecks der Ebene (siehe [`include_plane_half`]).
    Plane(PlaneId),
    /// Eine Achse des Ursprungs als Linie (Länge wie bei der Ebene).
    Axis(AxisId),
    /// Ein Konstruktionspunkt (Platzhalter, siehe [`PointRefId`]).
    ConstructionPoint(PointRefId),
    /// Der Ursprung (ein Punkt).
    Origin,
    /// Eine Kurve einer früheren Skizze, in Raumlage.
    SketchCurve { sketch: FeatureId, curve: CurveId },
}

/// Halbe Kantenlänge des Rechtecks einer einbezogenen Ebene und halbe Länge einer einbezogenen
/// Achse: wie die gezeichnete Konstruktionsebene der App 0,6 × Diagonale der sichtbaren Körper
/// (hier nach ihren Ecken), mindestens 25 mm. *(Annahme: die Randlinien des angezeigten
/// Rechtecks, Spec 5.)*
pub fn include_plane_half(solids: &[&Solid]) -> f64 {
    const MIN_HALF_MM: f64 = 25.0;
    const HALF_PER_DIAGONAL: f64 = 0.6;
    let diagonal = solids
        .iter()
        .flat_map(|s| s.vertices.iter().map(|(_, v)| v.point))
        .filter(|p| p.is_finite())
        .fold(None, |acc: Option<(DVec3, DVec3)>, p| match acc {
            None => Some((p, p)),
            Some((lo, hi)) => Some((lo.min(p), hi.max(p))),
        })
        .map_or(0.0, |(lo, hi)| (hi - lo).length());
    let half = (HALF_PER_DIAGONAL * diagonal).max(MIN_HALF_MM);
    if half.is_finite() { half } else { MIN_HALF_MM }
}

/// Platzhalter „Auf Fläche projizieren“ (Task 17 legt die Felder fest): Quellen und Ziele.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SurfaceProjection {
    pub sources: Vec<IncludeRef>,
    pub targets: Vec<IncludeRef>,
}

/// Platzhalter „Schnittkurve“ (Task 18 legt die Felder fest): Quellen und Ziele.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct IntersectionSpec {
    pub sources: Vec<IncludeRef>,
    pub targets: Vec<IncludeRef>,
}

impl IncludeRef {
    /// Features, aus denen der Verweis stammt und die vor der Skizze stehen müssen (feste
    /// Verweise wie bei Kanten). Quellskizzen und Konstruktionspunkte sind lose Verweise
    /// ([`IncludeRef::loose_origin`]).
    fn origins(&self, out: &mut Vec<FeatureId>) {
        match self {
            IncludeRef::Edge(e) | IncludeRef::Vertex(VertexRef { edge: e, .. }) => {
                out.push(FeatureId(e.a.origin));
                out.push(FeatureId(e.b.origin));
            }
            IncludeRef::FaceBoundary(f) => out.push(FeatureId(f.origin)),
            IncludeRef::Plane(PlaneId::Feature(id)) => out.push(*id),
            IncludeRef::Plane(PlaneId::Standard(_))
            | IncludeRef::Axis(_)
            | IncludeRef::ConstructionPoint(_)
            | IncludeRef::Origin
            | IncludeRef::SketchCurve { .. } => {}
        }
    }

    /// Lose Verweise: Quellskizze oder Konstruktionspunkt. Sie stehen nicht in
    /// [`FeatureKind::dependencies`]: Gelöscht oder hinter die Skizze geschoben ist der Bezug
    /// verloren (Warnung, Kurve bleibt, Spec 10), kein Fehler.
    fn loose_origin(&self) -> Option<FeatureId> {
        match self {
            IncludeRef::SketchCurve { sketch, .. } => Some(*sketch),
            IncludeRef::ConstructionPoint(id) => Some(*id),
            _ => None,
        }
    }
}

impl CopySource {
    /// Features, aus denen die Herkunft stammt (Marken der Flächen, Konstruktionsebenen); ohne
    /// die losen Verweise ([`CopySource::loose_origins`]).
    pub fn origins(&self) -> Vec<FeatureId> {
        match self {
            CopySource::Edge(e) => vec![FeatureId(e.a.origin), FeatureId(e.b.origin)],
            CopySource::Section { face, .. } | CopySource::Silhouette { face, .. } => {
                vec![FeatureId(face.origin)]
            }
            CopySource::Include(r) => {
                let mut out = Vec::new();
                r.origins(&mut out);
                out
            }
            CopySource::SketchCurve { .. } => Vec::new(),
            CopySource::OnSurface(SurfaceProjection { sources, targets })
            | CopySource::Intersection(IntersectionSpec { sources, targets }) => {
                let mut out = Vec::new();
                for r in sources.iter().chain(targets) {
                    r.origins(&mut out);
                }
                out
            }
        }
    }

    /// Lose Verweise der Herkunft: Quellskizzen (und Konstruktionspunkte). Nur Skizzen vor der
    /// Skizze im Verlauf gelten ([`sketch_source_allowed`]); sonst ist der Bezug verloren.
    pub fn loose_origins(&self) -> Vec<FeatureId> {
        match self {
            CopySource::Edge(_) | CopySource::Section { .. } | CopySource::Silhouette { .. } => {
                Vec::new()
            }
            CopySource::Include(r) => r.loose_origin().into_iter().collect(),
            CopySource::SketchCurve { sketch, .. } => vec![*sketch],
            CopySource::OnSurface(SurfaceProjection { sources, targets })
            | CopySource::Intersection(IntersectionSpec { sources, targets }) => sources
                .iter()
                .chain(targets)
                .filter_map(IncludeRef::loose_origin)
                .collect(),
        }
    }

    /// Eine der Arten aus Skizzen 10 (Format 20)?
    pub fn is_3d_kind(&self) -> bool {
        !matches!(
            self,
            CopySource::Edge(_) | CopySource::Section { .. } | CopySource::Silhouette { .. }
        )
    }
}

/// Darf die Skizze `source` Quelle der aktiven Skizze `active` sein (Spec 10)? Nur eine Skizze,
/// die im Verlauf vor der aktiven steht; damit sind gegenseitige Bezüge ausgeschlossen.
pub fn sketch_source_allowed(doc: &Document, active: FeatureId, source: FeatureId) -> bool {
    let (Some(a), Some(s)) = (doc.index_of(active), doc.index_of(source)) else {
        return false;
    };
    s < a && matches!(doc.features[s].kind, FeatureKind::Sketch(_))
}

/// Ziel einer Kopie: eine Skizzenkurve oder (bei Ecken, Konstruktionspunkten, dem Ursprung und
/// Schnittpunkten) ein fester Skizzenpunkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyTarget {
    Curve(CurveId),
    Point(PointId),
}

/// Projizierte oder einbezogene Skizzengeometrie mit ihrer Herkunft; sie folgt ihr bei jeder
/// Neuberechnung. Geht der Bezug verloren, bleibt sie wie gespeichert, und das Skizzen-Feature
/// bekommt eine Warnung (Skizzen 6, wie Fusion). `piece` ist der Stückindex in einer Gruppe mit
/// derselben Herkunft (0 bei allen alten Arten, in der Datei dann weggelassen).
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeCopy {
    pub target: CopyTarget,
    pub source: CopySource,
    pub piece: u32,
}

impl EdgeCopy {
    /// Kopie auf die Kurve `curve` (Stück 0).
    pub fn new(curve: CurveId, source: CopySource) -> EdgeCopy {
        EdgeCopy {
            target: CopyTarget::Curve(curve),
            source,
            piece: 0,
        }
    }

    /// Die Zielkurve; `None` bei einem Punkt als Ziel.
    pub fn curve(&self) -> Option<CurveId> {
        match self.target {
            CopyTarget::Curve(c) => Some(c),
            CopyTarget::Point(_) => None,
        }
    }

    /// Der Zielpunkt; `None` bei einer Kurve als Ziel.
    pub fn point(&self) -> Option<PointId> {
        match self.target {
            CopyTarget::Point(p) => Some(p),
            CopyTarget::Curve(_) => None,
        }
    }

    /// Die Kante, wenn die Herkunft eine Kante ist.
    pub fn edge(&self) -> Option<&EdgeRef> {
        match &self.source {
            CopySource::Edge(e) => Some(e),
            _ => None,
        }
    }
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

    /// Löst die Verknüpfung der projizierten Kurve `curve` mit ihrer Herkunft: Die Kurve folgt
    /// dem Körper nicht mehr, ihre `Fixed`-Werte bleiben. `true`, wenn es einen Eintrag gab.
    pub fn break_link(&mut self, curve: CurveId) -> bool {
        let before = self.copies.len();
        self.copies.retain(|c| c.target != CopyTarget::Curve(curve));
        self.copies.len() != before
    }

    /// Entfernt Projektionen, deren Kurve in der Skizze nicht mehr lebt; `true`, wenn sich
    /// etwas änderte.
    pub fn prune_copies(&mut self) -> bool {
        let before = self.copies.len();
        let sketch = &self.sketch;
        self.copies.retain(|c| match c.target {
            CopyTarget::Curve(id) => sketch.curve(id).is_some(),
            CopyTarget::Point(id) => sketch.point_xyz(id).is_ok(),
        });
        self.copies.len() != before
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
    pub fn label(&self) -> String {
        if self.is_chamfer() {
            tr!("feature-label-chamfer")
        } else {
            tr!("feature-label-fillet")
        }
    }

    /// Ausführliche Bezeichnung für Eigenschaften und Fenster.
    pub fn kind_label(&self) -> String {
        match self {
            BlendArt::Rundung { .. } => tr!("feature-label-fillet"),
            BlendArt::RundungVariabel { .. } => tr!("feature-label-fillet-variable"),
            BlendArt::FaseGleich { .. } => tr!("feature-label-chamfer-equal"),
            BlendArt::FaseZwei { .. } => tr!("feature-label-chamfer-two"),
            BlendArt::FaseWinkel { .. } => tr!("feature-label-chamfer-angle"),
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

/// Gewählte Profile einer Extrusion oder Rotation.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileSel {
    /// Altes Profil `n` (bisherige Extraktion, nur aus alten Dateien und in Tests); beim
    /// Berechnen über [`Sketch::migrate_profile_index`] auf die heutigen Flächen umgerechnet.
    Index(usize),
    /// Verweise auf Flächen der Skizze ([`planora_sketch::ExtractedProfiles::find`]);
    /// nebeneinanderliegende Flächen ergeben einen Körper, getrennte je einen (wie Fusion).
    Refs(Vec<ProfileRef>),
}

/// Art eines Features mit seinen Parametern (Maße in Millimetern).
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "few features per document; holding the sketch directly keeps the interface simple"
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
        profiles: ProfileSel,
        height: f64,
    },
    Revolve {
        sketch: FeatureId,
        profiles: ProfileSel,
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
    /// Konstruktionsebene (Skizzen 6): erzeugt keinen Körper, nur ein Koordinatensystem.
    Plane(crate::planes::PlaneFeature),
    /// Leinwand (Skizzen 9): Bild als Zeichenhilfe auf einem Träger; erzeugt keinen Körper.
    Canvas(crate::canvas::CanvasFeature),
}

impl FeatureKind {
    /// Bezeichnung (in der Sprache des Programms) der Feature-Art für die Oberfläche.
    pub fn label(&self) -> String {
        match self {
            FeatureKind::Sketch(_) => tr!("feature-label-sketch"),
            FeatureKind::Box { .. } => tr!("feature-label-box"),
            FeatureKind::Cylinder { .. } => tr!("feature-label-cylinder"),
            FeatureKind::Sphere { .. } => tr!("feature-label-sphere"),
            FeatureKind::Cone { .. } => tr!("feature-label-cone"),
            FeatureKind::Extrude { .. } => tr!("feature-label-extrude"),
            FeatureKind::Revolve { .. } => tr!("feature-label-revolve"),
            FeatureKind::Boolean { .. } => tr!("feature-label-boolean"),
            FeatureKind::Move { .. } => tr!("feature-label-move"),
            FeatureKind::Verrunden { art, .. } => art.label(),
            FeatureKind::MeshBody { .. } => tr!("feature-label-mesh"),
            FeatureKind::StepBody { .. } => tr!("feature-label-step"),
            FeatureKind::Plane(_) => tr!("feature-label-plane"),
            FeatureKind::Canvas(_) => tr!("feature-label-canvas"),
        }
    }

    /// Alle Features, auf die dieses verweist; aufsteigend sortiert, ohne Doppelte.
    pub fn dependencies(&self) -> Vec<FeatureId> {
        let mut deps = match self {
            FeatureKind::Sketch(s) => {
                let mut d = Vec::new();
                match &s.plane {
                    SketchPlane::Standard(_) => {}
                    SketchPlane::OnFace(r) => d.push(FeatureId(r.origin)),
                    SketchPlane::OnPlane(id) => d.push(*id),
                }
                for c in &s.copies {
                    d.extend(c.source.origins());
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
            FeatureKind::Plane(p) => p.dependencies(),
            FeatureKind::Canvas(c) => c.dependencies(),
        };
        deps.sort();
        deps.dedup();
        deps
    }

    /// Körper, die diese Art verbraucht (ersetzt): Ziel und Werkzeug einer Booleschen
    /// Operation, der Körper von Verschieben und Verrunden. Grundlage der Sichtbarkeit und der
    /// Körperspitzen ([`Document::body_tips`]).
    pub fn consumed_bodies(&self) -> Vec<BodyRef> {
        match self {
            FeatureKind::Boolean { target, tool, .. } => vec![*target, *tool],
            FeatureKind::Move { body, .. } | FeatureKind::Verrunden { body, .. } => vec![*body],
            FeatureKind::Sketch(_)
            | FeatureKind::Box { .. }
            | FeatureKind::Cylinder { .. }
            | FeatureKind::Sphere { .. }
            | FeatureKind::Cone { .. }
            | FeatureKind::Extrude { .. }
            | FeatureKind::Revolve { .. }
            | FeatureKind::MeshBody { .. }
            | FeatureKind::StepBody { .. }
            | FeatureKind::Plane(_)
            | FeatureKind::Canvas(_) => Vec::new(),
        }
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
    /// Warnung der Berechnung (siehe [`Feature::warning`]).
    pub(crate) warning: Option<String>,
    /// Koordinatensystem einer Konstruktionsebene (siehe [`Document::plane_frame`]).
    pub(crate) plane: Option<Frame>,
    /// Koordinatensystem des Trägers einer Leinwand (siehe [`Document::canvas_frame`]); bei
    /// verlorener Fläche die letzte Lage.
    pub(crate) canvas: Option<Frame>,
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
    /// Warnung der letzten Berechnung bei Status `Ok` (etwa verlorene Bezüge projizierter
    /// Skizzenkurven); wird nur gerechnet, nicht gespeichert und zählt nicht zum Stand.
    pub(crate) warning: Option<String>,
}

impl Feature {
    /// Die Formeln der Zahlenfelder (Feld → Formeltext).
    pub fn formulas(&self) -> &crate::formulas::Formulas {
        &self.formulas
    }

    /// Warnung der letzten Berechnung (nur bei Status `Ok`), etwa verlorene Bezüge projizierter
    /// Skizzenkurven.
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
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
    /// Bilder der Leinwände (Skizzen 9); mehrere Leinwände können dasselbe Bild benutzen.
    pub(crate) images: BTreeMap<crate::canvas::ImageId, Arc<crate::canvas::CanvasImage>>,
    /// Nächste zu vergebende Bildkennung (beginnt bei 1).
    pub(crate) next_image: u64,
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
    /// Was im Browser ausgeblendet ist (zählt nicht zum Modell).
    pub(crate) visibility: crate::visibility::Visibility,
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
            images: BTreeMap::new(),
            next_image: 1,
            drawings: Vec::new(),
            next_drawing: 1,
            drawing_edits: 0,
            project_info: None,
            parts_overrides: BTreeMap::new(),
            visibility: crate::visibility::Visibility::default(),
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
