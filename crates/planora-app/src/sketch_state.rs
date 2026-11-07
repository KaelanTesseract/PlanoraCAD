//! Skizzen-Modus ohne Oberfläche: Werkzeuge, Auswahl, Ziehen und Abhängigkeiten.
//!
//! Alle Methoden, die die Skizze ändern, sind atomar: Bei einem Fehler bleiben Skizze und
//! Sitzung (laufendes Werkzeug, Auswahl) unverändert, und `message` erklärt den Fehler.

use crate::formula_field::resolve;
use crate::msg::Msg;
use crate::sketch_cursor::{CursorOptions, DrawCursor};
use crate::sketch_grips::GripDrag;
use crate::sketch_input::{Created, DrawInput, FieldKind};
use crate::sketch_kinds::{PREVIEW_STEPS_PER_TURN, ToolKinds};
use planora_feature::{FeatureId, ParamScope, ParamUnit, SketchFeature, constraint_unit};
use planora_i18n::tr;
use planora_kernel::{Edge, Id, Solid};
use planora_math::DVec2;
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use planora_sketch::dimension::{DimPick, DimPlan};
use planora_sketch::model::{CurveId, CurveKind, MAX_SPLINE_POINTS, PointId, Sketch};
use planora_sketch::offset::{OffsetPlan, OffsetProblem};
use planora_sketch::shapes::{folded_over, sweep_crossed_half_turn};
use planora_sketch::snap::{SnapHit, SnapIndex};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use std::collections::BTreeSet;
use std::sync::Arc;

pub use planora_sketch::snap::Snap;

/// Schritte je Vollkreis für den Linienzug, gegen den der Auswahlrahmen prüft.
const BOX_STEPS_PER_TURN: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Point,
    Line,
    Rectangle,
    Circle,
    Arc,
    /// Fusion „Polygon“; die Art steht in [`SketchSession::kinds`].
    Polygon,
    /// Fusion „Langloch“; die Art steht in [`SketchSession::kinds`].
    Slot,
    Ellipse,
    Spline,
    ProjectEdge,
    /// Fusion „Stutzen“: Klick oder Ziehen löscht das Kurvenstück zwischen den nächsten
    /// Schnittpunkten.
    Trim,
    /// Fusion „Dehnen“: verlängert ein offenes Ende bis zur nächsten Kurve.
    Extend,
    /// Fusion „Brechen“: teilt eine Kurve an den Schnittpunkten um die Klickstelle.
    Break,
    /// Fusion „Skizzenrundung“ und „Skizzenfase“; die Art steht in [`SketchSession::kinds`].
    Fillet,
    /// Fusion „Versatz“.
    Offset,
    /// Fusion „Verschieben/Kopieren“.
    MoveCopy,
    /// Fusion „Skizzenmaßstab“.
    Scale,
    /// Fusion „Skizzenbemaßung“ (Skizzen 5): Objekte wählen, der Zeiger wählt die Art, ein Klick
    /// setzt das Maß und öffnet sein Feld ([`crate::sketch_dimension`]).
    Dimension,
    /// Fusion „Rechteckige Anordnung“ und „Kreisförmige Anordnung“ (Skizzen 5); die Art steht in
    /// [`SketchSession::kinds`], der Ablauf in [`crate::sketch_pattern`].
    Pattern,
    /// Befehl einer geometrischen Abhängigkeit „erst Befehl, dann Auswahl“ (Handtest Skizzen 5,
    /// Korrektur 4): Klicks wählen die Bezüge, sobald es reicht, entsteht die Abhängigkeit, und
    /// der Befehl bleibt für das nächste Paar ([`SketchSession::constraint_button`]).
    Constraint(ConstraintAction),
}

impl Tool {
    /// Werkzeuge unter „Ändern“ (Stutzen, Dehnen, Brechen, Rundung und Fase, Versatz,
    /// Verschieben/Kopieren, Maßstab): ohne offene Punkte, Esc beendet sie.
    pub fn is_modify(self) -> bool {
        matches!(
            self,
            Tool::Trim
                | Tool::Extend
                | Tool::Break
                | Tool::Fillet
                | Tool::Offset
                | Tool::MoveCopy
                | Tool::Scale
        )
    }
}

/// Zustand der Skizzenbemaßung ([`crate::sketch_dimension`]): die gewählten Objekte, das gesetzte
/// Maß, dessen Feld offen ist, und der Umschalter Ø/R der Vorschau.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DimToolState {
    pub picks: Vec<DimPick>,
    /// Gesetzt, noch nicht angelegt: Das Maßfeld im Skizzenfeld ist offen
    /// (`EditTarget::Planned`). Die Ansicht zeigt es an seinem Anker.
    pub plan: Option<DimPlan>,
    /// Ø/R umgeschaltet (Tab oder Knopf), solange gewählt wird.
    pub toggled: bool,
    /// Im Feld des gesetzten Maßes getippter Text (`None`: unverändert, es gilt der gemessene
    /// Wert). Das Skizzenfeld schreibt ihn in jedem Bild; ein Klick daneben legt damit an.
    pub text: Option<String>,
}

/// Stufe von Verschieben/Kopieren und Maßstab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TransformStage {
    /// Kurven und Punkte werden gewählt.
    #[default]
    Picking,
    /// Der Bezugspunkt wird gesetzt.
    SetBase,
    /// Die Griffe und Felder wirken.
    Active,
}

/// Die gewählte Menge von Verschieben/Kopieren und Maßstab.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TransformSet {
    pub curves: Vec<CurveId>,
    pub points: Vec<PointId>,
}

/// Zustand von Verschieben/Kopieren und Maßstab; die Logik steht in [`crate::sketch_grips`].
#[derive(Clone, Debug, PartialEq)]
pub struct TransformState {
    pub stage: TransformStage,
    pub set: TransformSet,
    pub base: Option<DVec2>,
    pub offset: DVec2,
    pub angle: f64,
    pub factor: f64,
    pub copy: bool,
    pub whole: bool,
    /// Die Auswahl vor dem Befehl: Abbrechen stellt sie wieder her.
    pub selection_before: Vec<Item>,
    /// `message` sagt gerade, dass die Bewegung nicht geht ([`SketchSession::update_transform_block`]).
    pub blocked_note: bool,
}

impl Default for TransformState {
    fn default() -> TransformState {
        TransformState {
            stage: TransformStage::default(),
            set: TransformSet::default(),
            base: None,
            offset: DVec2::ZERO,
            angle: 0.0,
            factor: 1.0,
            copy: false,
            whole: false,
            selection_before: Vec::new(),
            blocked_note: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Point(PointId),
    Curve(CurveId),
    Constraint(ConstraintId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConstraintAction {
    Coincident,
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Tangent,
    Equal,
    Fixed,
    Midpoint,
    Symmetric,
    PointOnCurve,
    Distance,
    DistancePointLine,
    Radius,
    Diameter,
    Angle,
    /// Fusion „Konzentrisch“: ein `Coincident` der beiden Mitten (Skizzen 5).
    Concentric,
    /// Fusion „Kollinear“.
    Collinear,
    /// Fusion „Glatt“ (G2; an einem Bogen bis Teilprojekt 7 nur G1).
    Smooth,
}

impl ConstraintAction {
    /// Name als gespeicherte Meldung (für Meldungen, die mit der Sprache wechseln).
    pub fn label_msg(self) -> Msg {
        use ConstraintAction::*;
        match self {
            Coincident => crate::msg!("sketch-constraint-coincident"),
            Horizontal => crate::msg!("sketch-constraint-horizontal"),
            Vertical => crate::msg!("sketch-constraint-vertical"),
            Parallel => crate::msg!("sketch-constraint-parallel"),
            Perpendicular => crate::msg!("sketch-constraint-perpendicular"),
            Tangent => crate::msg!("sketch-constraint-tangent"),
            Equal => crate::msg!("sketch-constraint-equal"),
            Fixed => crate::msg!("sketch-constraint-fixed"),
            Midpoint => crate::msg!("sketch-constraint-midpoint"),
            Symmetric => crate::msg!("sketch-constraint-symmetric"),
            PointOnCurve => crate::msg!("sketch-constraint-point-on-curve"),
            Distance => crate::msg!("sketch-constraint-distance"),
            DistancePointLine => crate::msg!("sketch-constraint-distance-point-line"),
            Radius => crate::msg!("sketch-constraint-radius"),
            Diameter => crate::msg!("sketch-constraint-diameter"),
            Angle => crate::msg!("sketch-constraint-angle"),
            Concentric => crate::msg!("sketch-constraint-concentric"),
            Collinear => crate::msg!("sketch-constraint-collinear"),
            Smooth => crate::msg!("sketch-constraint-smooth"),
        }
    }

    /// Name in der aktuellen Sprache.
    pub fn label(self) -> String {
        self.label_msg().text()
    }

    /// Ist das ein Maß (mit Zahlenwert)?
    pub fn is_dimension(self) -> bool {
        use ConstraintAction::*;
        matches!(
            self,
            Distance | DistancePointLine | Radius | Diameter | Angle
        )
    }

    /// Einheit, die eine Formel für dieses Maß ergeben muss (Winkel in Grad, sonst Länge).
    pub fn unit(self) -> ParamUnit {
        if self == ConstraintAction::Angle {
            ParamUnit::Angle
        } else {
            ParamUnit::Length
        }
    }
}

/// Meldung, wenn man in ein anzeigendes Maß eine Formel tippt.
pub fn driven_no_formula() -> String {
    tr!("sketch-driven-no-formula")
}

/// Zustand einer Skizze mit Formeln, die sich nicht lösen lässt (Plan P12): derselbe Text wie
/// der Fehlerstatus, den die Skizze beim Beenden bekommt. `None` ohne Formeln oder gelöst.
pub fn formula_problem(sf: &SketchFeature, report: Option<&SolveReport>) -> Option<Msg> {
    if sf.formulas.is_empty() {
        return None;
    }
    match report?.status {
        SolveStatus::Solved => None,
        SolveStatus::Conflicting => Some(crate::msg!("sketch-formula-conflict")),
        SolveStatus::NotConverged => Some(crate::msg!("sketch-formula-unsolvable")),
    }
}

/// Wert für die Skizze (Winkel in Bogenmaß) und Formel aus dem Text eines Maßfelds.
pub(crate) fn dimension_input(
    text: &str,
    unit: ParamUnit,
    scope: &ParamScope,
) -> Result<(f64, Option<String>), Msg> {
    let (v, formula) = resolve(text, unit, scope)?;
    let v = if unit == ParamUnit::Angle {
        v.to_radians()
    } else {
        v
    };
    Ok((v, formula))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Laufender Auswahlrahmen, in Bildschirmlagen.
#[derive(Clone, Copy, Debug)]
pub struct BoxSelect {
    pub start: DVec2,
    pub current: DVec2,
    /// Umschalt beim Beginn: ergänzt die Auswahl, statt sie zu ersetzen.
    pub extend: bool,
}

impl BoxSelect {
    /// Von links nach rechts: Fenster (durchgezogen); sonst Kreuzung (gestrichelt).
    pub fn is_window(&self) -> bool {
        self.start.x <= self.current.x
    }
}

/// Liegt `p` im achsenparallelen Rechteck `lo`–`hi` (Rand gehört dazu)?
fn in_rect(p: DVec2, lo: DVec2, hi: DVec2) -> bool {
    p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
}

/// Hat die Strecke `a`–`b` einen Punkt im Rechteck `lo`–`hi`? (Abschneiden nach Liang–Barsky)
fn segment_hits_rect(a: DVec2, b: DVec2, lo: DVec2, hi: DVec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (from, step, min, max) in [(a.x, d.x, lo.x, hi.x), (a.y, d.y, lo.y, hi.y)] {
        if step == 0.0 {
            if from < min || from > max {
                return false;
            }
        } else {
            let (enter, leave) = ((min - from) / step, (max - from) / step);
            t0 = t0.max(enter.min(leave));
            t1 = t1.min(enter.max(leave));
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Laufender Zug an einer Kurve. Die Ziele rechnen immer vom Start aus (`Start + Zeiger − Griff`),
/// so springt nichts auf den Zeiger.
#[derive(Clone, Debug)]
pub struct CurveDrag {
    /// Wo die Kurve gegriffen wurde.
    pub grab: DVec2,
    /// Die bewegten Punkte (ohne Doppelte) mit ihrer Lage beim Greifen.
    pub start: Vec<(PointId, DVec2)>,
    /// Einzelner Kreis am Rand: zieht den Radius; dazu Griffabstand von der Mitte minus Radius.
    pub rim: Option<(CurveId, f64)>,
    /// Die Skizze beim Greifen: Kommt der Zug ohne Änderung dorthin zurück, gilt wieder sie.
    /// `None`, sobald mitten im Zug noch etwas anderes geändert wurde.
    origin: Option<Sketch>,
    /// Lösungsbericht zu `origin`: Was schon dort scheiterte, hält den Zug nicht auf.
    baseline: SolveReport,
    /// Stand nach dem letzten Bild; ein anderer heißt, dass mitten im Zug noch etwas geändert wurde.
    frame_revision: u64,
}

impl CurveDrag {
    /// Hat seit dem letzten Bild etwas anderes die Skizze geändert (etwa „Konstruktion“ per Taste),
    /// gibt es keinen Stand mehr, zu dem der Zug zurückkehren dürfte, und der Vergleich gilt ab jetzt.
    fn note_foreign_change(&mut self, sk: &Sketch) {
        if sk.revision() != self.frame_revision {
            self.origin = None;
            self.baseline = sk.clone().solve();
        }
    }
}

/// Macht ein Bild des Zugs die Lage schlechter als beim Greifen? Ein schon vorhandener Widerspruch
/// in einer fremden Gruppe zählt nicht.
fn frame_is_worse(report: &SolveReport, baseline: &SolveReport) -> bool {
    report.drag_failed
        || (status_ok(baseline) && !status_ok(report))
        || report
            .conflicting
            .iter()
            .any(|c| !baseline.conflicting.contains(c))
}

/// Gleichen sich die Werte zweier Stände derselben Skizze bis auf die Toleranz?
fn same_values(a: &Sketch, b: &Sketch) -> bool {
    let linear = a.tolerance().linear;
    a.variables().len() == b.variables().len()
        && a.variables()
            .iter()
            .zip(b.variables())
            .all(|(x, y)| (x - y).abs() <= linear)
}

/// Laufender Zug auf dem letzten Punkt des Linienzugs: wird ein tangentialer Bogen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineArcDrag {
    pub start: PointId,
    pub prev: CurveId,
}

/// Zustand des geöffneten Skizzen-Modus.
#[derive(Clone, Debug)]
pub struct SketchSession {
    /// Das bearbeitete Skizzen-Feature (die Sitzung arbeitet auf seinem Entwurf).
    pub sketch: FeatureId,
    pub tool: Tool,
    /// Je Form die zuletzt gewählte Art; gilt für diese Sitzung.
    pub kinds: ToolKinds,
    pub selection: Vec<Item>,
    /// Punkte des laufenden Werkzeugs.
    pub pending: Vec<PointId>,
    /// Noch nicht angelegte Eckpunkte (Rechteck, Ellipse).
    pub pending_pos: Vec<DVec2>,
    /// Fang der noch nicht angelegten Eckpunkte in `pending_pos` (Rechteck: erste Ecke, Ellipse:
    /// Ende der großen Achse); die Anbindung folgt beim Abschluss. Bogen: Fang der Mitte (nur eine
    /// frei gesetzte Mitte rückt für die Tangente zurecht).
    pub pending_snaps: Vec<SnapHit>,
    /// Schon gewählte Kurven einer Art, die Kurven wählt (`picks_curves`).
    pub pending_curves: Vec<CurveId>,
    /// Zuletzt abgeschlossene Seitenzahl eines Polygons; ein abgebrochenes zählt nicht.
    pub polygon_sides: usize,
    /// Laufender Zug auf dem letzten Punkt des Linienzugs.
    pub line_arc: Option<LineArcDrag>,
    /// Bogen aus dem Linienzug, an dem die nächste Linie tangential weiterläuft.
    pub line_after_arc: Option<CurveId>,
    pub cursor: Option<DVec2>,
    /// Strg und Raster für den Zeiger ([`resolve_cursor`](Self::resolve_cursor)).
    pub cursor_opts: CursorOptions,
    /// Aufgelöster Zeiger dieses Bilds ([`update_cursor`](Self::update_cursor)).
    pub draw_cursor: Option<DrawCursor>,
    /// Fang-Index zum letzten gesehenen Stand der Skizze (nur in Zeichenwerkzeugen gebaut).
    pub snap_index: Option<Arc<SnapIndex>>,
    /// Beim Zeichnen automatisch waagerecht, senkrecht, parallel, rechtwinklig und tangential
    /// ergänzen (die Anbindung an Fangziele bleibt auch ohne).
    pub auto_constrain: bool,
    /// Letzte Linie des laufenden Linienzugs; was mit ihr zusammenhängt, ist kein Richtungsbezug.
    pub line_chain: Option<CurveId>,
    /// Bogen: Kurve, an der der gesetzte Anfang tangential weiterläuft; entsteht beim Abschluss.
    pub auto_tangent: Option<CurveId>,
    /// Felder am Zeiger mit den getippten Werten (siehe [`crate::sketch_input`]).
    pub input: DrawInput,
    /// Was der letzte Klick fertig angelegt hat.
    pub created: Option<Created>,
    pub last_report: Option<SolveReport>,
    /// Meldung zum letzten Schritt; gespeichert als Grund und erst beim Zeichnen übersetzt
    /// ([`message_text`](Self::message_text)).
    pub message: Option<Msg>,
    pub dragging: Option<PointId>,
    /// Laufender Zug an einer Kurve; gegriffene Punkte stehen in [`dragging`](Self::dragging).
    pub curve_drag: Option<CurveDrag>,
    /// Laufender Auswahlrahmen.
    pub box_select: Option<BoxSelect>,
    /// Die rechte Taste hat den Rahmen abgebrochen: Bis alle Maustasten los sind, ist kein Klick
    /// ein Klick (egui meldet sonst das Loslassen der rechten oder linken Taste als solchen, und
    /// der leerte die Auswahl).
    pub box_aborted: bool,
    /// Ein Drücken hat die offene Liste der Arten geschlossen: Der nächste Klick zählt einfach,
    /// auch wenn egui ihn mit dem verschluckten zum Doppelklick zusammenzählt.
    pub click_after_flyout: bool,
    /// Zählt die Sprünge durch Rückgängig/Wiederholen ([`SketchSession::after_history_jump`]);
    /// Eingabefelder der Oberfläche, die zu einem älteren Zählerstand gehören, schließen.
    pub history_jumps: u64,
    /// Vorschau unter dem Mauszeiger (Stutzen und Brechen: das Stück, Dehnen: der Weg zum neuen
    /// Ende), je Bild neu bestimmt ([`update_hover`](Self::update_hover)).
    pub hover_preview: Option<Vec<DVec2>>,
    /// Name des nächsten Schritts für Rückgängig, wenn ein Werkzeug ihn besser kennt als der
    /// Vergleich der Stände (Stutzen, Dehnen, Brechen); der Verlauf nimmt und leert ihn.
    pub edit_label: Option<Msg>,
    /// Letzte Stelle beim Stutzen durch Ziehen (`None`: kein Zug läuft).
    pub trim_drag: Option<DVec2>,
    /// Zählt die gelungenen Änderungen durch Stutzen, Dehnen und Brechen, mit dem Werkzeug der
    /// letzten (für „Letztes Skizzenwerkzeug wiederholen“, auch wenn Esc es im selben Bild
    /// beendet).
    pub modify_edits: (u64, Option<Tool>),
    /// Rundung/Fase: die zuletzt gerundete Kurve und ihr Radius; die nächste Rundung derselben
    /// Sitzung schließt mit dem zweiten Klick ab.
    pub fillet_run: Option<(CurveId, f64)>,
    /// Versatz: die ganze tangentiale Kette nehmen (Vorgabe).
    pub offset_chain: bool,
    /// Klicklagen der gewählten Kurven von Rundung und Fase (zählen nicht in `kind_stage`).
    pub pick_points: Vec<DVec2>,
    /// Fangradius zu `pick_points`.
    pub pick_radius: f64,
    /// Klicklagen der im Auswählen gewählten Kurven (Skizzen 5): Glatt setzt am Ende an, das der
    /// Klickstelle am nächsten liegt. Ein Klick gehört nur zu der Auswahl, die er gemacht hat.
    pub curve_picks: CurvePicks,
    /// Verschieben/Kopieren und Maßstab.
    pub transform: TransformState,
    /// Laufender Zug an einem Griff.
    pub grip_drag: Option<GripDrag>,
    /// Skizzenbemaßung.
    pub dim: DimToolState,
    /// Rechteckige und Kreisförmige Anordnung.
    pub pattern: crate::sketch_pattern::PatternToolState,
}

/// Was vor einer Änderung gesichert wird, um sie bei einem Fehler zurückzunehmen.
struct Backup {
    sketch: Sketch,
    pending: Vec<PointId>,
    pending_pos: Vec<DVec2>,
    pending_snaps: Vec<SnapHit>,
    pending_curves: Vec<CurveId>,
    line_arc: Option<LineArcDrag>,
    line_after_arc: Option<CurveId>,
    line_chain: Option<CurveId>,
    auto_tangent: Option<CurveId>,
    input: DrawInput,
    selection: Vec<Item>,
    last_report: Option<SolveReport>,
    fillet_run: Option<(CurveId, f64)>,
    offset_chain: bool,
    pick_points: Vec<DVec2>,
    pick_radius: f64,
    curve_picks: CurvePicks,
    transform: TransformState,
    grip_drag: Option<GripDrag>,
    dim: DimToolState,
    pattern: crate::sketch_pattern::PatternToolState,
}

/// Klicklagen der Kurven, die das Auswählen per Klick gewählt hat, und die Auswahl gleich nach
/// dem letzten solchen Klick. Hat sich die Auswahl seitdem anders geändert (Liste, Rahmen, Kette,
/// Werkzeugwechsel, Rückgängig, Löschen …), gelten die Klicks nicht mehr.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurvePicks {
    pub at: Vec<(CurveId, DVec2)>,
    pub selection: Vec<Item>,
}

/// Auswahl nach Art getrennt (jeweils in Auswahlreihenfolge).
#[derive(Default)]
struct Sorted {
    points: Vec<PointId>,
    lines: Vec<CurveId>,
    /// Kreise und Bögen
    rounds: Vec<CurveId>,
    /// Ellipsen und Splines
    others: Vec<CurveId>,
    /// Kurven aller Arten in Auswahlreihenfolge.
    curves: Vec<CurveId>,
}

/// Fehler einer Bibliothek als Meldung (der Text kommt von dort, bis die Bibliotheken übersetzt
/// sind).
pub(crate) fn err(e: impl std::fmt::Display) -> Msg {
    Msg::Raw(e.to_string())
}

fn status_ok(r: &SolveReport) -> bool {
    r.status == SolveStatus::Solved
}

/// Hinweis, wenn die Skizze nach einer erlaubten Änderung weiterhin nicht gelöst ist.
fn unsolved_hint(report: Option<&SolveReport>) -> Option<Msg> {
    match report.map(|r| r.status) {
        Some(SolveStatus::Conflicting) => Some(crate::msg!("sketch-still-conflicting")),
        Some(SolveStatus::NotConverged) => Some(crate::msg!("sketch-still-unsolved")),
        _ => None,
    }
}

impl SketchSession {
    /// Die Meldung in der aktuellen Sprache.
    pub fn message_text(&self) -> Option<String> {
        self.message.as_ref().map(Msg::text)
    }

    pub fn new(sketch: FeatureId) -> SketchSession {
        SketchSession {
            sketch,
            tool: Tool::Select,
            kinds: ToolKinds::default(),
            selection: Vec::new(),
            pending: Vec::new(),
            pending_pos: Vec::new(),
            pending_snaps: Vec::new(),
            pending_curves: Vec::new(),
            polygon_sides: 6,
            line_arc: None,
            line_after_arc: None,
            cursor: None,
            cursor_opts: CursorOptions::default(),
            draw_cursor: None,
            snap_index: None,
            auto_constrain: true,
            line_chain: None,
            auto_tangent: None,
            input: DrawInput::default(),
            created: None,
            last_report: None,
            message: None,
            dragging: None,
            curve_drag: None,
            box_select: None,
            box_aborted: false,
            click_after_flyout: false,
            history_jumps: 0,
            hover_preview: None,
            edit_label: None,
            trim_drag: None,
            modify_edits: (0, None),
            fillet_run: None,
            offset_chain: true,
            pick_points: Vec::new(),
            curve_picks: CurvePicks::default(),
            pick_radius: 0.0,
            transform: TransformState::default(),
            grip_drag: None,
            dim: DimToolState::default(),
            pattern: Default::default(),
        }
    }

    /// Wechselt das Werkzeug; ein laufendes Werkzeug wird abgebrochen, ein laufendes Ziehen
    /// wie mit [`end_drag`](Self::end_drag) beendet.
    pub fn set_tool(&mut self, sk: &mut Sketch, tool: Tool) {
        self.end_drag(sk);
        self.box_select = None;
        self.trim_drag = None;
        self.hover_preview = None;
        self.fillet_run = None;
        if self.transform.blocked_note || self.pattern.blocked_note {
            self.message = None;
        }
        self.transform = TransformState::default();
        self.grip_drag = None;
        // Muster: Plan, Abwahl und Griff gehören zum Werkzeug, nichts bleibt.
        self.pattern = Default::default();
        // Die halbe Auswahl eines Abhängigkeitsbefehls gehört ihm; ein anderes Werkzeug nimmt sie
        // nicht mit (ein anderer Abhängigkeitsknopf entscheidet selbst, `constraint_button`).
        if matches!(self.tool, Tool::Constraint(_)) && !matches!(tool, Tool::Constraint(_)) {
            self.selection.clear();
        }
        self.tool = tool;
        // Ein Werkzeugwechsel beendet die Klick-Auswahl.
        self.drop_curve_picks();
        self.draw_cursor = None;
        self.clear_pending();
        // Versatz: Eine Vorauswahl mit Kurven ist schon die Kette, es geht gleich mit dem Abstand weiter.
        if tool == Tool::Offset {
            let curves: Vec<CurveId> = self
                .selection
                .iter()
                .filter_map(|i| match *i {
                    Item::Curve(c) if sk.curve(c).is_some() => Some(c),
                    _ => None,
                })
                .collect();
            self.pending_curves = offset_preselection(sk, curves);
        }
        // Verschieben/Kopieren: Eine Vorauswahl ist schon die Menge.
        if self.runs_transform() {
            self.transform.selection_before = self.selection.clone();
            self.begin_transform(sk);
        }
        // Muster: Eine Vorauswahl mit Kurven ist schon die Vorlage.
        if tool == Tool::Pattern {
            self.pattern.selection_before = self.selection.clone();
            self.begin_pattern(sk);
        }
    }

    /// Vergisst, was das laufende Werkzeug schon gesetzt und getippt hat.
    pub(crate) fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_pos.clear();
        self.pending_snaps.clear();
        self.pending_curves.clear();
        self.line_arc = None;
        self.line_after_arc = None;
        self.line_chain = None;
        self.auto_tangent = None;
        self.pick_points.clear();
        self.input = DrawInput::default();
        // Bemaßen: Wahl und gesetztes Maß (sein Feld schließt) entfallen, nichts bleibt.
        self.dim = DimToolState::default();
    }

    /// Wählt `tool` wie der Klick auf seinen Knopf in der Leiste (und sein Tastenkürzel): ist
    /// es schon gewählt, bleibt alles (auch ein laufender Linienzug), sonst wie
    /// [`set_tool`](Self::set_tool).
    pub fn choose_tool(&mut self, sk: &mut Sketch, tool: Tool) {
        if self.tool != tool {
            self.set_tool(sk, tool);
        }
    }

    /// Nach Rückgängig/Wiederholen in der Skizze (R10): `sk` ist der zurückgeholte Entwurf.
    /// Laufendes Werkzeug (offene Punkte und Ecken), Auswahl, Ziehen und Meldung gehören zum
    /// alten Stand und entfallen, ebenso ein offenes Maßfeld (über `history_jumps`).
    /// Freiheitsgrade und Färbung kommen aus einem frischen Lösen einer Kopie, damit der Entwurf
    /// selbst (und sein Stand) unverändert bleibt. Werkzeug und Mauszeiger bleiben.
    pub fn after_history_jump(&mut self, sk: &Sketch) {
        self.clear_pending();
        self.draw_cursor = None;
        self.selection.clear();
        self.drop_curve_picks();
        self.dragging = None;
        self.curve_drag = None;
        self.box_select = None;
        self.trim_drag = None;
        self.hover_preview = None;
        self.fillet_run = None;
        self.transform = TransformState::default();
        self.grip_drag = None;
        self.pattern.reset_after_jump();
        self.edit_label = None;
        self.message = None;
        self.last_report = Some(sk.clone().solve());
        self.history_jumps += 1;
    }

    /// Fang für die Auswahl an der Stelle `at`: Punkte vor Kurven (nur [`Snap::Point`],
    /// [`Snap::Curve`] und [`Snap::None`]; beim Zeichnen gilt
    /// [`resolve_cursor`](Self::resolve_cursor)).
    pub fn snap(&self, sk: &Sketch, at: DVec2, radius: f64) -> Snap {
        if let Some(p) = sk.pick_point(at, radius) {
            Snap::Point(p)
        } else if let Some(c) = sk.pick_curve(at, radius) {
            Snap::Curve(c)
        } else {
            Snap::None
        }
    }

    // ---- Atomarität ---------------------------------------------------------------------

    fn backup(&self, sk: &Sketch) -> Backup {
        Backup {
            sketch: sk.clone(),
            pending: self.pending.clone(),
            pending_pos: self.pending_pos.clone(),
            pending_snaps: self.pending_snaps.clone(),
            pending_curves: self.pending_curves.clone(),
            line_arc: self.line_arc,
            line_after_arc: self.line_after_arc,
            line_chain: self.line_chain,
            auto_tangent: self.auto_tangent,
            input: self.input.clone(),
            selection: self.selection.clone(),
            last_report: self.last_report.clone(),
            fillet_run: self.fillet_run,
            offset_chain: self.offset_chain,
            pick_points: self.pick_points.clone(),
            curve_picks: self.curve_picks.clone(),
            pick_radius: self.pick_radius,
            transform: self.transform.clone(),
            grip_drag: self.grip_drag,
            dim: self.dim.clone(),
            pattern: self.pattern.clone(),
        }
    }

    fn restore(&mut self, sk: &mut Sketch, b: Backup) {
        *sk = b.sketch;
        self.pending = b.pending;
        self.pending_pos = b.pending_pos;
        self.pending_snaps = b.pending_snaps;
        self.pending_curves = b.pending_curves;
        self.line_arc = b.line_arc;
        self.line_after_arc = b.line_after_arc;
        self.line_chain = b.line_chain;
        self.auto_tangent = b.auto_tangent;
        self.input = b.input;
        self.selection = b.selection;
        self.last_report = b.last_report;
        self.fillet_run = b.fillet_run;
        self.offset_chain = b.offset_chain;
        self.pick_points = b.pick_points;
        self.curve_picks = b.curve_picks;
        self.pick_radius = b.pick_radius;
        self.transform = b.transform;
        self.grip_drag = b.grip_drag;
        self.dim = b.dim;
        self.pattern = b.pattern;
    }

    /// Führt `f` atomar aus: Bei einem Fehler wird alles zurückgesetzt und die Meldung in
    /// `message` abgelegt. Vorher wird `message` geleert.
    pub(crate) fn atomic<T>(
        &mut self,
        sk: &mut Sketch,
        f: impl FnOnce(&mut Self, &mut Sketch, &Sketch) -> Result<T, Msg>,
    ) -> Result<T, String> {
        self.message = None;
        let b = self.backup(sk);
        match f(self, sk, &b.sketch) {
            Ok(v) => {
                // Hat der Befehl Muster aufgelöst (Löschen einer Vorlage, Stutzen, Rundung …, auch
                // in Folge), sagt die Meldung es (Entscheidung 6 des Plans Skizzen 5).
                let dissolved = b.sketch.pattern_count().saturating_sub(sk.pattern_count());
                if dissolved > 0 {
                    self.add_note(crate::msg!("sketch-pattern-dissolved", n = dissolved));
                }
                Ok(v)
            }
            Err(e) => {
                self.restore(sk, b);
                let text = e.text();
                self.message = Some(e);
                Err(text)
            }
        }
    }

    /// Hängt `note` an die Meldung an (oder setzt sie, wenn keine ansteht).
    pub(crate) fn add_note(&mut self, note: Msg) {
        self.message = Some(match self.message.take() {
            None => note,
            Some(Msg::Join(mut all)) => {
                all.push(note);
                Msg::Join(all)
            }
            Some(m) => Msg::Join(vec![m, note]),
        });
    }

    /// Löst die Skizze und merkt sich den Bericht. Macht die Änderung die Skizze widersprüchlich
    /// oder unlösbar, ist das ein Fehler (siehe [`check_change`]).
    pub(crate) fn solve_checked(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
    ) -> Result<SolveReport, Msg> {
        let before = self.last_report.take();
        let report = sk.solve();
        self.last_report = Some(report.clone());
        if let Some(problem) = check_change(&report, before.as_ref(), prior, None) {
            return Err(match problem {
                SolveStatus::Conflicting => crate::msg!("sketch-change-conflict"),
                _ => crate::msg!("sketch-change-unsolved"),
            });
        }
        self.warn_if_unsolved();
        Ok(report)
    }

    /// Ist die Skizze nach einer erlaubten Änderung weiterhin nicht gelöst (der Fehler bestand
    /// schon vorher), wird darauf hingewiesen, sofern keine andere Meldung ansteht.
    fn warn_if_unsolved(&mut self) {
        if self.message.is_some() {
            return;
        }
        self.message = unsolved_hint(self.last_report.as_ref());
    }

    // ---- Werkzeuge ----------------------------------------------------------------------

    /// Klick mit dem aktiven Werkzeug an der Stelle `at` (Skizzeneinheiten); `radius` ist der
    /// Fangradius, `extend` erweitert bzw. schaltet die Auswahl um.
    pub fn click(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        extend: bool,
    ) -> Result<(), String> {
        if !at.is_finite() || !radius.is_finite() || radius < 0.0 {
            let e = crate::msg!("sketch-invalid-position");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        }
        self.cursor = Some(at);
        self.created = None;
        self.pick_radius = radius;
        let cur = self.resolve_cursor(sk, at, radius);
        self.atomic(sk, |se, sk, prior| {
            if se.planned_kind().is_some() {
                return se.click_kind(sk, &cur, prior);
            }
            match se.tool {
                Tool::Select => {
                    se.select_at(sk, at, radius, extend);
                    Ok(())
                }
                Tool::Point => se.click_point(sk, &cur, prior),
                Tool::Line => se.click_line(sk, &cur, prior),
                Tool::Rectangle => se.click_rectangle(sk, &cur, prior),
                Tool::Circle => se.click_circle(sk, &cur, prior),
                Tool::Arc => se.click_arc(sk, &cur, prior),
                Tool::Ellipse => se.click_ellipse(sk, &cur, prior),
                Tool::Spline => se.click_spline(sk, &cur, prior),
                // Haben nur geplante Arten (oben); die Kantenkopie steuert die Oberfläche über
                // `project_edge`.
                Tool::Polygon | Tool::Slot | Tool::ProjectEdge => Ok(()),
                Tool::Trim | Tool::Extend | Tool::Break => se.click_modify(sk, at, radius, prior),
                Tool::Offset => se.click_offset(sk, &cur, prior),
                Tool::MoveCopy | Tool::Scale => {
                    se.click_transform(sk, &cur);
                    Ok(())
                }
                // Rundung und Fase laufen über ihre Arten (`click_kind`).
                Tool::Fillet => Ok(()),
                Tool::Dimension => {
                    se.click_dimension(sk, at, radius);
                    Ok(())
                }
                Tool::Pattern => {
                    se.click_pattern(sk, &cur);
                    Ok(())
                }
                Tool::Constraint(action) => se.click_constraint(sk, action, at, radius),
            }
        })
    }

    /// Klick des Versatzes: Ohne Kette wählt er die Kurve unter dem Zeiger (mit
    /// [`offset_chain`](Self::offset_chain) ihre Kette), sonst legt er die Kopie aus dem Plan der
    /// Vorschau an. Danach bleibt das Werkzeug aktiv, ohne Kette. Unmögliches ergibt eine Meldung,
    /// und die Stufe bleibt ([`atomic`](Self::atomic)).
    fn click_offset(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if self.pending_curves.is_empty() {
            let Snap::Curve(c) = cur.snap else {
                return Ok(());
            };
            self.pending_curves = if self.offset_chain {
                offset_chain_of(sk, c)?
            } else {
                vec![c]
            };
            return Ok(());
        }
        let plan = match self.offset_plan_at(sk, cur.raw) {
            Ok(plan) => plan,
            // Kein Abstand (Zeiger auf der Kette): Dieser Klick sagt nichts.
            Err(OffsetProblem::Degenerate) => return Ok(()),
            Err(problem) => return Err(offset_problem_message(problem)),
        };
        let ids = sk.add_offset(&plan).map_err(err)?;
        self.solve_checked(sk, prior)?;
        if !ids.tied {
            self.message = Some(crate::msg!("sketch-offset-not-tied"));
        } else if plan.dropped > 0 {
            self.message = Some(crate::msg!("sketch-offset-dropped", n = plan.dropped));
        }
        let created = Created::Offset(ids);
        self.modify_edits = (self.modify_edits.0 + 1, Some(self.tool));
        self.edit_label = Some(created.undo_label());
        self.created = Some(created);
        self.selection.clear();
        self.drop_curve_picks();
        self.clear_pending();
        Ok(())
    }

    /// Plan des Versatzes für den Zeiger `at`, für Vorschau und Klick. Betrag: das Feld, sonst der
    /// Abstand des Zeigers zur nächsten Kette. Seite: je Kette die des Zeigers, ein negativer Wert
    /// wechselt sie. `offset_plan` nimmt einen Abstand für alle Ketten; darum läuft eine Kette, bei
    /// der der Zeiger auf der anderen Seite liegt als bei der nächsten, andersherum.
    pub fn offset_plan_at(&self, sk: &Sketch, at: DVec2) -> Result<OffsetPlan, OffsetProblem> {
        let mut chains = sk.ordered_chains(&self.pending_curves)?;
        let sides: Vec<Option<f64>> = chains.iter().map(|c| sk.offset_side(c, at)).collect();
        let nearest = sides
            .iter()
            .flatten()
            .copied()
            .min_by(|a, b| a.abs().total_cmp(&b.abs()));
        let toward = nearest.map_or(1.0, f64::signum);
        let distance = match self.input.locked(FieldKind::OffsetDistance) {
            Some(typed) => toward * typed,
            None => nearest.ok_or(OffsetProblem::Degenerate)?,
        };
        for (chain, side) in chains.iter_mut().zip(sides) {
            if side.is_some_and(|s| s.signum() != toward) {
                chain.items.reverse();
                for item in &mut chain.items {
                    item.1 = !item.1;
                }
            }
        }
        sk.offset_plan(&chains, distance)
    }

    /// Vorschau des Versatzes als Linienzüge: dieselbe Rechnung wie der Klick. `None`, solange
    /// keine Kette gewählt ist oder der Versatz nicht geht.
    pub fn offset_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>> {
        if self.tool != Tool::Offset || self.pending_curves.is_empty() {
            return None;
        }
        let plan = self.offset_plan_at(sk, at).ok()?;
        Some(plan.outline(PREVIEW_STEPS_PER_TURN))
    }

    /// Abstand des Zeigers `at` zur nächsten gewählten Kette.
    pub(crate) fn offset_pointer_distance(&self, sk: &Sketch, at: DVec2) -> Option<f64> {
        let chains = sk.ordered_chains(&self.pending_curves).ok()?;
        chains
            .iter()
            .filter_map(|c| sk.offset_side(c, at))
            .map(f64::abs)
            .min_by(f64::total_cmp)
    }

    /// Stutzen, Dehnen oder Brechen der Kurve unter `at`. Ohne Kurve dort passiert nichts;
    /// Dehnen ohne Ziel meldet `sketch-extend-no-hit` und ändert nichts.
    fn click_modify(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let Some(c) = sk.pick_curve(at, radius) else {
            return Ok(());
        };
        let label = match self.tool {
            Tool::Trim => {
                sk.trim(c, at).map_err(err)?;
                crate::msg!("undo-trim")
            }
            Tool::Extend => {
                if !sk.extend(c, at).map_err(err)? {
                    return Err(crate::msg!("sketch-extend-no-hit"));
                }
                crate::msg!("undo-extend")
            }
            Tool::Break => {
                if sk.break_curve(c, at).map_err(err)?.len() < 2 {
                    return Ok(());
                }
                crate::msg!("undo-break")
            }
            _ => return Ok(()),
        };
        self.solve_checked(sk, prior)?;
        self.forget_removed(sk);
        self.hover_preview = None;
        self.edit_label = Some(label);
        self.modify_edits = (self.modify_edits.0 + 1, Some(self.tool));
        Ok(())
    }

    /// Auswahl und offene Punkte ohne die Elemente, die es nicht mehr gibt.
    pub(crate) fn forget_removed(&mut self, sk: &Sketch) {
        self.drop_curve_picks();
        self.selection.retain(|i| match *i {
            Item::Point(p) => sk.point_xy(p).is_ok(),
            Item::Curve(c) => sk.curve(c).is_some(),
            Item::Constraint(k) => sk.constraint(k).is_some(),
        });
        self.pending.retain(|&p| sk.point_xy(p).is_ok());
        self.forget_removed_curves(sk);
    }

    /// Gewählte Kurven samt ihren Klickstellen, der Lauf der Rundung und die Menge von
    /// Verschieben/Kopieren ohne das, was es nicht mehr gibt. Bleibt von der Menge nichts, wird wieder
    /// gewählt.
    fn forget_removed_curves(&mut self, sk: &Sketch) {
        let mut curves = self.pending_curves.iter();
        self.pick_points
            .retain(|_| curves.next().is_some_and(|&c| sk.curve(c).is_some()));
        self.pending_curves.retain(|&c| sk.curve(c).is_some());
        if self.fillet_run.is_some_and(|(c, _)| sk.curve(c).is_none()) {
            self.fillet_run = None;
        }
        let set = &mut self.transform.set;
        set.curves.retain(|&c| sk.curve(c).is_some());
        set.points.retain(|&p| sk.point_xy(p).is_ok());
        if set.curves.is_empty()
            && set.points.is_empty()
            && self.transform.stage != TransformStage::Picking
        {
            self.transform.stage = TransformStage::Picking;
            self.grip_drag = None;
        }
    }

    /// Vorschau unter dem Mauszeiger `at` (siehe [`hover_preview`](Self::hover_preview)):
    /// nur bei Stutzen, Dehnen und Brechen.
    pub fn update_hover(&mut self, sk: &Sketch, at: Option<DVec2>, radius: f64) {
        self.hover_preview = at
            .filter(|a| {
                a.is_finite() && matches!(self.tool, Tool::Trim | Tool::Extend | Tool::Break)
            })
            .and_then(|at| {
                let c = sk.pick_curve(at, radius)?;
                match self.tool {
                    Tool::Extend => sk.extend_preview(c, at),
                    _ => Some(sk.trim_piece(c, at)?.polyline),
                }
            })
            .filter(|pl| pl.len() >= 2);
    }

    /// Stutzen durch Ziehen: beginnt bei `at` (die Stelle des Drückens) und stutzt dort.
    pub fn begin_trim_drag(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) {
        if self.tool != Tool::Trim || !at.is_finite() {
            return;
        }
        self.trim_drag = Some(at);
        self.trim_along(sk, at, at, radius);
    }

    /// Stutzen durch Ziehen bis `at`: jede Kurve, die der Weg seit der letzten Stelle überfährt,
    /// wird an der überfahrenen Stelle gestutzt. Alles zusammen wird ein Schritt, weil der
    /// Verlauf erst nach dem Loslassen der Maustaste fortgeschrieben wird.
    pub fn trim_drag_to(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) {
        let Some(from) = self.trim_drag else {
            return;
        };
        if self.tool != Tool::Trim || !at.is_finite() {
            self.trim_drag = None;
            return;
        }
        self.trim_along(sk, from, at, radius);
        self.trim_drag = Some(at);
    }

    /// Beendet das Stutzen durch Ziehen.
    pub fn end_trim_drag(&mut self) {
        self.trim_drag = None;
    }

    /// Stutzt jede Kurve, die die Strecke `from`–`to` berührt (Proben im Abstand von höchstens
    /// `radius`). `from` selbst wird nur geprüft, wenn `from == to` (Beginn des Zugs); sonst
    /// war es schon die letzte Probe des vorigen Bilds, und eine zweite Probe dort träfe nach
    /// dem Stutzen das nächstgelegene andere Stück, das der Zug nie überfahren hat. Scheitert
    /// ein Stück, steht der Grund in `message`; die übrigen werden trotzdem gestutzt.
    fn trim_along(&mut self, sk: &mut Sketch, from: DVec2, to: DVec2, radius: f64) {
        /// Höchstens so viele Proben je Bild (ein sehr schneller Zug über eine kleine Skizze).
        const MAX_SAMPLES: usize = 400;
        let len = from.distance(to);
        let (first, n) = if len == 0.0 {
            (0, 0)
        } else if radius > 0.0 {
            (1, ((len / radius).ceil() as usize).clamp(1, MAX_SAMPLES))
        } else {
            (1, 1)
        };
        let mut failed = None;
        for k in first..=n {
            let p = if n == 0 {
                from
            } else {
                from.lerp(to, k as f64 / n as f64)
            };
            if sk.pick_curve(p, radius).is_none() {
                continue;
            }
            if self.click(sk, p, radius, false).is_err() {
                failed = self.message.take();
            }
        }
        if failed.is_some() {
            self.message = failed;
        }
        self.cursor = Some(to);
    }

    fn select_at(&mut self, sk: &Sketch, at: DVec2, radius: f64, extend: bool) {
        self.forget_stale_picks();
        let item = match self.snap(sk, at, radius) {
            Snap::Point(p) => Some(Item::Point(p)),
            Snap::Curve(c) => Some(Item::Curve(c)),
            _ => None,
        };
        match (item, extend) {
            (Some(i), true) => {
                if let Some(k) = self.selection.iter().position(|&s| s == i) {
                    self.selection.remove(k);
                } else {
                    self.selection.push(i);
                }
            }
            (Some(i), false) => self.selection = vec![i],
            (None, true) => {}
            (None, false) => self.selection.clear(),
        }
        match item {
            Some(Item::Curve(c)) => self.note_curve_pick(c, at),
            _ => self.sync_picks(),
        }
    }

    /// Merkt sich die Klicklage `at` auf der Kurve `c` (die letzte gilt), nachdem ein Klick die
    /// Auswahl geändert hat.
    fn note_curve_pick(&mut self, c: CurveId, at: DVec2) {
        self.curve_picks.at.retain(|(k, _)| *k != c);
        self.curve_picks.at.push((c, at));
        self.sync_picks();
    }

    /// Nach einer Auswahländerung durch einen Klick: Die Auswahl ist die der Klicks, Klicklagen
    /// abgewählter Kurven entfallen.
    fn sync_picks(&mut self) {
        let selection = &self.selection;
        self.curve_picks
            .at
            .retain(|(c, _)| selection.contains(&Item::Curve(*c)));
        self.curve_picks.selection = self.selection.clone();
    }

    /// Vergisst alle Klicklagen. Jede Auswahländerung außer einem Klick im Auswählen
    /// ([`select_at`](Self::select_at), [`begin_curve_drag`](Self::begin_curve_drag)) ruft das;
    /// [`forget_stale_picks`](Self::forget_stale_picks) fängt Stellen, die es vergessen.
    pub(crate) fn drop_curve_picks(&mut self) {
        self.curve_picks = CurvePicks::default();
    }

    /// Hat sich die Auswahl seit dem letzten Klick anders geändert, gelten die Klicklagen nicht
    /// mehr (ein Klick gehört nur zu der Auswahl, die er gemacht hat).
    pub(crate) fn forget_stale_picks(&mut self) {
        if self.curve_picks.selection != self.selection {
            self.drop_curve_picks();
        }
    }

    /// Klicklage der gewählten Kurve `c`, wenn sie per Klick gewählt wurde und die Auswahl seitdem
    /// nur durch Klicks geändert wurde.
    fn pick_of(&self, c: CurveId) -> Option<DVec2> {
        if self.curve_picks.selection != self.selection || !self.selection.contains(&Item::Curve(c))
        {
            return None;
        }
        self.curve_picks
            .at
            .iter()
            .rev()
            .find(|(k, _)| *k == c)
            .map(|(_, at)| *at)
    }

    fn click_point(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if cur.point(sk.tolerance().linear).is_some() {
            return Ok(());
        }
        self.attach(sk, None, cur)?;
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_line(&mut self, sk: &mut Sketch, cur: &DrawCursor, prior: &Sketch) -> Result<(), Msg> {
        let p = self.attach(sk, None, cur)?;
        let Some(&start) = self.pending.first() else {
            self.pending = vec![p];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let line = sk.add_line(start, p).map_err(err)?;
        self.solve_checked(sk, prior)?;
        for auto in &cur.autos {
            self.try_auto(sk, auto.kind(line));
        }
        self.line_chain = Some(line);
        self.line_after_arc = None;
        self.pending = vec![p];
        self.created = Some(Created::Line(line));
        Ok(())
    }

    /// Versucht eine automatische Abhängigkeit an einer Kopie: Sie entfällt still (`false`), wenn
    /// die Skizze vorher nicht gelöst war, danach nicht gelöst ist, die neue überzählig wäre oder
    /// ein Punkt sich um mehr als `10·tol.linear` bewegen würde.
    pub fn try_auto(&mut self, sk: &mut Sketch, kind: ConstraintKind) -> bool {
        if !self.last_report.as_ref().is_some_and(status_ok) {
            return false;
        }
        let mut attempt = sk.clone();
        let Ok(id) = attempt.add_constraint(kind) else {
            return false;
        };
        let report = attempt.solve();
        let limit = 10.0 * sk.tolerance().linear;
        let moved = sk
            .points()
            .any(|(p, _)| match (sk.point_xy(p), attempt.point_xy(p)) {
                (Ok(a), Ok(b)) => a.distance(b) > limit,
                _ => true,
            });
        if !status_ok(&report) || report.redundant.contains(&id) || moved {
            return false;
        }
        *sk = attempt;
        self.last_report = Some(report);
        true
    }

    fn click_rectangle(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let Some(&corner) = self.pending_pos.first() else {
            self.pending_pos = vec![cur.at];
            self.pending_snaps = vec![cur.remember(tol)];
            return Ok(());
        };
        let mut hits: Vec<DrawCursor> = self
            .pending_snaps
            .iter()
            .take(1)
            .map(|&hit| DrawCursor::remembered(hit, sk))
            .collect();
        hits.push(cur.clone());
        let rect = sk.add_rectangle(corner, cur.at).map_err(err)?;
        self.pending_pos.clear();
        self.pending_snaps.clear();
        // Das Rechteck legt eigene Ecken an (sortiert); gebunden wird die Ecke an der Lage.
        self.bind_own(sk, &rect.points, &hits)?;
        self.solve_checked(sk, prior)?;
        self.created = Some(Created::Rectangle(rect));
        Ok(())
    }

    fn click_circle(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let Some(&center) = self.pending.first() else {
            let c = self.attach(sk, None, cur)?;
            self.pending = vec![c];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let r = sk.point_xy(center).map_err(err)?.distance(cur.at);
        if r <= sk.tolerance().linear {
            return Err(crate::msg!("sketch-radius-positive"));
        }
        let circle = sk.add_circle(center, r).map_err(err)?;
        self.bind_rim(sk, circle, cur)?;
        self.pending.clear();
        self.solve_checked(sk, prior)?;
        self.created = Some(Created::Circle(circle));
        Ok(())
    }

    fn click_arc(&mut self, sk: &mut Sketch, cur: &DrawCursor, prior: &Sketch) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        match *self.pending.as_slice() {
            [] => {
                let c = self.attach(sk, None, cur)?;
                self.pending = vec![c];
                self.pending_snaps = vec![cur.remember(tol)];
            }
            [center] => {
                let s = self.attach(sk, None, cur)?;
                if s == center
                    || sk
                        .point_xy(s)
                        .map_err(err)?
                        .distance(sk.point_xy(center).map_err(err)?)
                        <= tol
                {
                    return Err(crate::msg!("sketch-arc-start-center"));
                }
                self.pending.push(s);
                self.plan_arc_tangent(sk, center, s, cur)?;
            }
            [center, start, ..] => {
                let m = sk.point_xy(center).map_err(err)?;
                let s = sk.point_xy(start).map_err(err)?;
                let r = s.distance(m);
                let dir = cur.at - m;
                if dir.length() <= tol {
                    return Err(crate::msg!("sketch-arc-end-center"));
                }
                let on_start = || crate::msg!("sketch-arc-end-on-start");
                let end = match cur.point(tol) {
                    Some(p) if p == start => return Err(on_start()),
                    Some(p) if p != center => p,
                    _ => {
                        // Das Ende liegt auf dem Radius; es bindet nur, wenn es dort das
                        // Fangziel trifft.
                        let q = m + dir.normalize() * r;
                        if q.distance(s) <= tol {
                            return Err(on_start());
                        }
                        let on_radius = DrawCursor {
                            at: q,
                            ..cur.clone()
                        };
                        self.attach(sk, None, &on_radius)?
                    }
                };
                if sk.point_xy(end).map_err(err)?.distance(s) <= tol {
                    return Err(on_start());
                }
                let arc = sk.add_arc(center, start, end).map_err(err)?;
                self.clear_pending();
                self.solve_checked(sk, prior)?;
                for auto in &cur.autos {
                    self.try_auto(sk, auto.kind(arc));
                }
                self.created = Some(Created::Arc(arc));
                return Ok(());
            }
        }
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_ellipse(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let Some(&center) = self.pending.first() else {
            let c = self.attach(sk, None, cur)?;
            self.pending = vec![c];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let m = sk.point_xy(center).map_err(err)?;
        let Some(&major_end) = self.pending_pos.first() else {
            if (cur.at - m).length() <= tol {
                return Err(crate::msg!("sketch-major-axis-positive"));
            }
            self.pending_pos = vec![cur.at];
            self.pending_snaps = vec![cur.remember(tol)];
            return Ok(());
        };
        let major = major_end - m;
        let len = major.length();
        if len <= tol {
            return Err(crate::msg!("sketch-major-axis-positive"));
        }
        let minor = (major / len).perp_dot(cur.at - m).abs().min(len);
        if minor <= tol {
            return Err(crate::msg!("sketch-minor-axis-positive"));
        }
        let first = self
            .pending_snaps
            .first()
            .map(|&hit| DrawCursor::remembered(hit, sk));
        let ellipse = sk.add_ellipse(center, major, minor).map_err(err)?;
        for c in first.iter().chain([cur]) {
            self.bind_rim(sk, ellipse, c)?;
        }
        self.clear_pending();
        self.solve_checked(sk, prior)?;
        self.created = Some(Created::Ellipse(ellipse));
        Ok(())
    }

    fn click_spline(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if self.pending.len() >= MAX_SPLINE_POINTS {
            return Err(crate::msg!("sketch-spline-max", max = MAX_SPLINE_POINTS));
        }
        let p = self.attach(sk, None, cur)?;
        if self.pending.contains(&p) {
            return Err(crate::msg!("sketch-spline-point-twice"));
        }
        self.pending.push(p);
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    /// Schließt das laufende Werkzeug ab: legt den Spline an bzw. beendet den Linienzug. Eine
    /// geplante Art schließt nur ihr letzter Klick ab: Doppelklick und Enter lassen die Lagen.
    pub fn finish_tool(&mut self, sk: &mut Sketch) -> Result<(), String> {
        if self.planned_kind().is_some() {
            return Ok(());
        }
        // Verschieben/Kopieren: Beim Wählen geht es weiter zu den Griffen, beim Bezugspunkt zurück.
        // Den Abschluss macht `finish_transform`, er braucht auch die Formeln.
        if self.runs_transform() {
            self.message = None;
            match self.transform.stage {
                TransformStage::Picking => self.leave_picking(sk),
                TransformStage::SetBase => self.transform.stage = TransformStage::Active,
                TransformStage::Active => {}
            }
            return Ok(());
        }
        // Muster: Beim Wählen geht es weiter zu Feldern und Griffen; den Abschluss macht
        // `finish_pattern`, er braucht auch die Formeln.
        if self.tool == Tool::Pattern {
            self.message = None;
            if self.pattern.stage == TransformStage::Picking {
                self.leave_pattern_picking(sk);
            }
            return Ok(());
        }
        // Versatz: Eingabe schließt ab wie ein Klick.
        if self.tool == Tool::Offset && !self.pending_curves.is_empty() {
            let raw = self.draw_cursor.as_ref().map(|c| c.raw).or(self.cursor);
            return match raw {
                Some(raw) => self.click(sk, raw, self.pick_radius, false),
                None => Ok(()),
            };
        }
        if self.tool != Tool::Spline {
            self.message = None;
            self.clear_pending();
            return Ok(());
        }
        self.atomic(sk, |se, sk, prior| {
            if se.pending.len() < 2 {
                return Err(crate::msg!("sketch-spline-min"));
            }
            sk.add_spline(&se.pending).map_err(err)?;
            se.pending.clear();
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Bricht das laufende Werkzeug ab; im Auswahlwerkzeug wird die Auswahl geleert. Ein
    /// laufendes Ziehen wird wie mit [`end_drag`](Self::end_drag) beendet (der Punkt bleibt,
    /// wo er ist). Stutzen, Dehnen und Brechen enden ganz (zurück zur Auswahl). Ein laufender
    /// Rahmen fällt allein weg, die alte Auswahl bleibt; ebenso ein laufender Zug auf dem letzten
    /// Punkt, der Linienzug bleibt, und ein laufender Griff, das Werkzeug bleibt. Verschieben/Kopieren
    /// hinterlässt auch in der Auswahl nichts. Die Skizzenbemaßung verwirft zuerst ein offenes Maß
    /// (gewählter Bezug oder gesetztes Maß) und endet erst, wenn nichts mehr offen ist. Ein
    /// Abhängigkeitsbefehl ([`Tool::Constraint`]) endet ganz, ohne seine halbe Auswahl.
    pub fn cancel(&mut self, sk: &mut Sketch) {
        if self.grip_drag.is_some() {
            self.abort_grip();
            return;
        }
        if self.pattern.drag.is_some() {
            self.abort_pattern_grip();
            return;
        }
        if self.box_select.take().is_some() || self.line_arc.take().is_some() {
            return;
        }
        // Muster: Esc und Rechtsklick lassen nichts zurück, auch keine Auswahl.
        if self.tool == Tool::Pattern {
            self.selection = std::mem::take(&mut self.pattern.selection_before);
            self.drop_curve_picks();
            self.set_tool(sk, Tool::Select);
            self.forget_removed(sk);
            return;
        }
        // Bemaßen: Ist nichts offen (kein Bezug gewählt, kein Maß gesetzt), beendet Esc das
        // Werkzeug wie in Fusion; sonst fällt unten nur das offene Maß weg, das Werkzeug bleibt.
        let dim_idle =
            self.tool == Tool::Dimension && self.dim.picks.is_empty() && self.dim.plan.is_none();
        self.end_drag(sk);
        self.clear_pending();
        // Abhängigkeitsbefehl: Esc und Rechtsklick beenden ihn ganz, die halbe Auswahl fällt weg.
        if matches!(self.tool, Tool::Constraint(_)) {
            self.selection.clear();
            self.drop_curve_picks();
            self.set_tool(sk, Tool::Select);
            return;
        }
        if dim_idle {
            self.set_tool(sk, Tool::Select);
            return;
        }
        if self.tool == Tool::Select {
            self.selection.clear();
            self.drop_curve_picks();
        }
        if self.runs_transform() {
            self.selection = self.transform.selection_before.clone();
            self.drop_curve_picks();
            self.forget_removed(sk);
        }
        // Stutzen, Dehnen und Brechen haben nichts Offenes: Esc beendet sie wie in Fusion (auch
        // mitten im Ziehen; was schon gestutzt ist, bleibt).
        if self.tool.is_modify() {
            self.set_tool(sk, Tool::Select);
        }
    }

    // ---- Rahmen und Kette ----------------------------------------------------------------

    /// Liegt bei `at` nichts, was man greifen oder klicken könnte? Nur dort beginnt ein Rahmen.
    /// Dieselbe Frage wie in [`begin_drag`](Self::begin_drag): [`snap`](Self::snap).
    pub fn is_free_at(&self, sk: &Sketch, at: DVec2, radius: f64) -> bool {
        matches!(self.snap(sk, at, radius), Snap::None)
    }

    /// Beginnt im Auswahlwerkzeug oder beim Wählen von Verschieben/Kopieren und Maßstab den Rahmen an
    /// der Bildschirmlage `at`.
    pub fn begin_box(&mut self, at: DVec2, extend: bool) {
        if (self.tool == Tool::Select || self.transform_picking()) && at.is_finite() {
            self.box_select = Some(BoxSelect {
                start: at,
                current: at,
                extend,
            });
        }
    }

    /// Führt den Rahmen zur Bildschirmlage `at`.
    pub fn box_to(&mut self, at: DVec2) {
        if let Some(b) = self.box_select.as_mut().filter(|_| at.is_finite()) {
            b.current = at;
        }
    }

    /// Schließt den Rahmen und wählt, was er erfasst ([`select_rect`](Self::select_rect)).
    pub fn end_box(&mut self, sk: &Sketch, project: &dyn Fn(DVec2) -> Option<DVec2>) {
        if let Some(b) = self.box_select.take() {
            self.select_rect(sk, b.start, b.current, b.extend, project);
        }
    }

    /// Bricht den Rahmen oder den Zug auf dem letzten Punkt mit der rechten Taste ab, siehe
    /// [`box_aborted`](Self::box_aborted).
    pub fn abort_box(&mut self) {
        if self.box_select.take().is_some() || self.line_arc.take().is_some() {
            self.box_aborted = true;
        }
    }

    /// Wählt nach einem Rahmen zwischen den Bildschirmlagen `a` und `b` (`project`: Skizze →
    /// Bildschirm; `None` hinter der Kamera). `a.x <= b.x` ist ein Fenster: Es nimmt Kurven, deren
    /// Linienzug ganz im Rahmen liegt. Sonst nimmt die Kreuzung auch Kurven mit einer Strecke, die
    /// den Rahmen schneidet; eine Strecke mit einem Ende hinter der Kamera zählt nicht.
    /// Von den Punkten zählen nur freie (ohne Kurve), damit jede Abhängigkeit zur Auswahl passt.
    /// `extend` ergänzt, ohne etwas abzuwählen; sonst ersetzt der Rahmen die Auswahl.
    pub fn select_rect(
        &mut self,
        sk: &Sketch,
        a: DVec2,
        b: DVec2,
        extend: bool,
        project: &dyn Fn(DVec2) -> Option<DVec2>,
    ) {
        let window = a.x <= b.x;
        let (lo, hi) = (a.min(b), a.max(b));
        let on_screen = |p: DVec2| project(p).filter(|q| q.is_finite());
        let mut found = Vec::new();
        for (c, _) in sk.curves() {
            let Some(line) = sk.curve_polyline(c, BOX_STEPS_PER_TURN) else {
                continue;
            };
            let line: Vec<Option<DVec2>> = line.into_iter().map(on_screen).collect();
            let hit = if window {
                line.iter().all(|p| p.is_some_and(|p| in_rect(p, lo, hi)))
            } else {
                line.windows(2).any(|w| match (w[0], w[1]) {
                    (Some(p), Some(q)) => segment_hits_rect(p, q, lo, hi),
                    _ => false,
                })
            };
            if hit {
                found.push(Item::Curve(c));
            }
        }
        let on_curves: BTreeSet<PointId> = sk.curves().flat_map(|(_, c)| c.kind.points()).collect();
        for (p, _) in sk.points().filter(|(p, _)| !on_curves.contains(p)) {
            if sk
                .point_xy(p)
                .ok()
                .and_then(on_screen)
                .is_some_and(|q| in_rect(q, lo, hi))
            {
                found.push(Item::Point(p));
            }
        }
        self.take_selection(found, extend);
        self.sync_picked(sk);
    }

    /// Wählt die zusammenhängende Kette um `c` ([`Sketch::connected_chain`]); `extend` ergänzt.
    pub fn select_chain(&mut self, sk: &Sketch, c: CurveId, extend: bool) {
        let chain = sk.connected_chain(c);
        self.take_selection(chain.into_iter().map(Item::Curve).collect(), extend);
        self.sync_picked(sk);
    }

    /// Setzt die Auswahl auf `items` oder ergänzt sie, ohne etwas doppelt aufzunehmen.
    fn take_selection(&mut self, items: Vec<Item>, extend: bool) {
        self.message = None;
        // Rahmen und Kette wählen ohne Klicklage, auch wenn die Auswahl gleich bleibt.
        self.drop_curve_picks();
        if !extend {
            self.selection.clear();
        }
        for item in items {
            if !self.selection.contains(&item) {
                self.selection.push(item);
            }
        }
    }

    // ---- Ziehen -------------------------------------------------------------------------

    /// Läuft ein Zug an einem Punkt oder einer Kurve?
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some() || self.curve_drag.is_some()
    }

    /// Beginnt im Auswahlwerkzeug den Zug an dem, was unter `at` liegt: Punkt vor Kurve
    /// ([`snap`](Self::snap)); wo nichts liegt, beginnt der Rahmen
    /// ([`is_free_at`](Self::is_free_at)).
    pub fn begin_drag(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) -> bool {
        if self.tool != Tool::Select || !at.is_finite() {
            return false;
        }
        self.dragging = None;
        self.curve_drag = None;
        match self.snap(sk, at, radius) {
            Snap::Point(p) => self.dragging = Some(p),
            Snap::Curve(c) => self.begin_curve_drag(sk, c, at),
            _ => {}
        }
        self.is_dragging()
    }

    /// Greift die Kurve `c` bei `at`. Außerhalb der Auswahl ersetzt sie diese, sonst ziehen alle
    /// gewählten Kurven und Punkte mit. Ein einzelner Kreis zieht am Rand den Radius.
    fn begin_curve_drag(&mut self, sk: &Sketch, c: CurveId, at: DVec2) {
        self.forget_stale_picks();
        if !self.selection.contains(&Item::Curve(c)) {
            self.selection = vec![Item::Curve(c)];
            self.note_curve_pick(c, at);
        }
        let mut start: Vec<(PointId, DVec2)> = Vec::new();
        let mut moved_items = 0;
        for item in &self.selection {
            let points = match *item {
                Item::Curve(k) => sk.curve(k).map_or(Vec::new(), |d| d.kind.points()),
                Item::Point(p) => vec![p],
                Item::Constraint(_) => continue,
            };
            moved_items += 1;
            for p in points {
                if !start.iter().any(|&(q, _)| q == p)
                    && let Ok(xy) = sk.point_xy(p)
                {
                    start.push((p, xy));
                }
            }
        }
        let rim = match (moved_items, sk.curve(c).map(|d| &d.kind), sk.radius(c)) {
            (1, Some(&CurveKind::Circle { center, .. }), Some(r)) => {
                sk.point_xy(center).ok().map(|m| (c, m.distance(at) - r))
            }
            _ => None,
        };
        self.curve_drag = Some(CurveDrag {
            grab: at,
            start,
            rim,
            origin: Some(sk.clone()),
            baseline: sk.clone().solve(),
            frame_revision: sk.revision(),
        });
    }

    /// Führt den Zug nach `at`. Punkt: Scheitert der Solver, bleibt die Skizze, und `message`
    /// erklärt es. Kurve: siehe [`curve_drag_to`](Self::curve_drag_to).
    pub fn drag_to(&mut self, sk: &mut Sketch, at: DVec2) {
        if let Some(mut drag) = self.curve_drag.take() {
            drag.note_foreign_change(sk);
            self.curve_drag_to(sk, &drag, at);
            drag.frame_revision = sk.revision();
            self.curve_drag = Some(drag);
            return;
        }
        let Some(point) = self.dragging else {
            return;
        };
        self.cursor = Some(at);
        match sk.drag(&[Drag { point, target: at }]) {
            Ok(report) => {
                self.message = (!status_ok(&report)).then(|| crate::msg!("sketch-drag-blocked"));
                self.last_report = Some(report);
            }
            Err(e) => self.message = Some(err(e)),
        }
    }

    /// Ein Bild des Zugs an einer Kurve. Macht es die Lage schlechter ([`frame_is_worse`]), gilt
    /// wieder der Stand vor diesem Bild, und `message` erklärt es. Bewegt es nichts (ohne Bewegung des Zeigers, voll
    /// bestimmt, Radius mit Maß), bleibt die Skizze bitgleich und ohne neuen Stand, damit daraus
    /// kein Schritt wird.
    fn curve_drag_to(&mut self, sk: &mut Sketch, drag: &CurveDrag, at: DVec2) {
        self.cursor = Some(at);
        let before = sk.clone();
        let result = match drag.rim {
            Some((c, offset)) => {
                let center = sk.curve(c).and_then(|d| match d.kind {
                    CurveKind::Circle { center, .. } => sk.point_xy(center).ok(),
                    _ => None,
                });
                center.map_or(Err(SketchError::UnknownObject), |m| {
                    sk.drag_radius(c, m.distance(at) - offset)
                })
            }
            None => {
                let delta = at - drag.grab;
                let drags: Vec<Drag> = drag
                    .start
                    .iter()
                    .map(|&(point, from)| Drag {
                        point,
                        target: from + delta,
                    })
                    .collect();
                sk.drag(&drags)
            }
        };
        match result {
            Ok(report) if !frame_is_worse(&report, &drag.baseline) => {
                if same_values(sk, &before) {
                    *sk = before;
                } else {
                    self.last_report = Some(report);
                }
                self.message = None;
            }
            _ => {
                *sk = before;
                self.message = Some(crate::msg!("sketch-drag-blocked"));
            }
        }
    }

    /// Beendet den Zug. Ein volles Lösen holt dabei die Diagnose nach, die während des Ziehens
    /// für die gezogene Gruppe entfällt (unveränderte Gruppen kommen aus dem Zwischenspeicher des
    /// Solvers und kosten nichts). Der Zug an einer Kurve benennt den Schritt. Steht danach alles
    /// wie beim Greifen, gilt wieder die Skizze von damals (gleicher Stand), und es entsteht
    /// kein Schritt.
    pub fn end_drag(&mut self, sk: &mut Sketch) {
        if let Some(mut drag) = self.curve_drag.take() {
            drag.note_foreign_change(sk);
            match drag.origin.filter(|origin| same_values(sk, origin)) {
                Some(origin) => {
                    *sk = origin;
                    self.last_report = Some(drag.baseline);
                }
                None => {
                    self.last_report = Some(sk.solve());
                    self.edit_label = Some(crate::msg!("undo-sketch-move"));
                }
            }
        }
        if self.dragging.take().is_some() {
            self.last_report = Some(sk.solve());
        }
    }

    // ---- Abhängigkeiten -----------------------------------------------------------------

    fn sorted(&self, sk: &Sketch) -> Option<Sorted> {
        sort_items(sk, &self.selection)
    }

    /// Abhängigkeiten, die zur aktuellen Auswahl passen.
    pub fn available_actions(&self, sk: &Sketch) -> Vec<ConstraintAction> {
        actions_for(sk, &self.selection)
    }

    /// Knopf (oder Taste) einer Abhängigkeit im Skizzenfeld. Passt die Auswahl, entsteht sie
    /// sofort wie bisher (läuft dabei ein anderer Abhängigkeitsbefehl, endet er). Sonst startet
    /// der Befehl „erst Befehl, dann Auswahl“ wie in Fusion ([`Tool::Constraint`]): Eine Auswahl,
    /// die ein Anfang sein kann, zählt als erste Bezüge (mit ihren Klicklagen); eine andere fällt
    /// weg, und die Meldung sagt, warum sie nicht passt. Maße (mit Wertfeld) gehen nur über
    /// [`apply`](Self::apply).
    pub fn constraint_button(
        &mut self,
        sk: &mut Sketch,
        action: ConstraintAction,
    ) -> Result<(), String> {
        self.forget_stale_picks();
        let running = matches!(self.tool, Tool::Constraint(_));
        if action.is_dimension() || self.available_actions(sk).contains(&action) {
            self.apply(sk, action, None)?;
            if running {
                self.selection.clear();
                self.drop_curve_picks();
                // Die Meldung des Anlegens (etwa „überzählig“) bleibt stehen.
                let note = self.message.take();
                self.set_tool(sk, Tool::Select);
                self.message = note;
            }
            return Ok(());
        }
        self.end_drag(sk);
        let keep = constraint_fit(sk, action, &self.selection) == Fit::Partial;
        let refusal = (!keep && !self.selection.is_empty())
            .then(|| constraint_mismatch(sk, action, &self.selection));
        let (selection, picks) = (self.selection.clone(), self.curve_picks.clone());
        self.set_tool(sk, Tool::Constraint(action));
        if keep {
            // Der Werkzeugwechsel ersetzt die Auswahl nicht: Ihre Klicklagen gelten weiter.
            self.selection = selection;
            self.curve_picks = picks;
        } else {
            self.selection.clear();
            self.drop_curve_picks();
        }
        self.message = refusal;
        Ok(())
    }

    /// Klick im Abhängigkeitsbefehl: wählt den Bezug unter dem Zeiger (Punkt vor Kurve; passt der
    /// Punkt nicht, aber die Kurve, die Kurve) oder wählt einen schon gewählten wieder ab. Passt
    /// keiner, ist die Meldung der Grund, und die Auswahl bleibt ([`atomic`](Self::atomic)).
    /// Reicht die Auswahl, entsteht die Abhängigkeit (ein Schritt), die Auswahl leert sich, und
    /// der Befehl bleibt. Scheitert das Anlegen (Widerspruch …), bleibt die Meldung, und die
    /// Auswahl leert sich ebenso, damit das nächste Paar frisch beginnt.
    fn click_constraint(
        &mut self,
        sk: &mut Sketch,
        action: ConstraintAction,
        at: DVec2,
        radius: f64,
    ) -> Result<(), Msg> {
        self.forget_stale_picks();
        // Was nicht Punkt oder Kurve ist (etwa ein Eintrag der Liste), ist kein Bezug.
        let before = self.selection.len();
        self.selection.retain(|i| match *i {
            Item::Point(p) => sk.point_xy(p).is_ok(),
            Item::Curve(c) => sk.curve(c).is_some(),
            Item::Constraint(_) => false,
        });
        if self.selection.len() != before {
            self.drop_curve_picks();
        }
        let candidates: Vec<Item> = [
            sk.pick_point(at, radius).map(Item::Point),
            sk.pick_curve(at, radius).map(Item::Curve),
        ]
        .into_iter()
        .flatten()
        .collect();
        let Some(&first) = candidates.first() else {
            return Ok(());
        };
        let with = |item: Item| {
            let mut items = self.selection.clone();
            items.push(item);
            items
        };
        let chosen = candidates.iter().copied().find(|&i| {
            self.selection.contains(&i) || constraint_fit(sk, action, &with(i)) != Fit::No
        });
        let Some(item) = chosen else {
            return Err(constraint_mismatch(sk, action, &with(first)));
        };
        if let Some(k) = self.selection.iter().position(|&s| s == item) {
            self.selection.remove(k);
            self.sync_picks();
            return Ok(());
        }
        self.selection.push(item);
        match item {
            Item::Curve(c) => self.note_curve_pick(c, at),
            _ => self.sync_picks(),
        }
        if constraint_fit(sk, action, &self.selection) == Fit::Complete {
            let _ = self.apply(sk, action, None);
            self.selection.clear();
            self.drop_curve_picks();
        }
        Ok(())
    }

    /// Baut die Abhängigkeit aus der Auswahl (Maße mit Platzhalterwert 0).
    fn build(&self, sk: &Sketch, action: ConstraintAction) -> Result<ConstraintKind, Msg> {
        use ConstraintAction as A;
        use ConstraintKind as K;
        if self.selection.is_empty() {
            return Err(crate::msg!("sketch-nothing-selected"));
        }
        if !self.available_actions(sk).contains(&action) {
            return Err(constraint_mismatch(sk, action, &self.selection));
        }
        let s = self
            .sorted(sk)
            .ok_or_else(|| crate::msg!("sketch-selection-invalid"))?;
        let p = &s.points;
        let c = &s.curves;
        Ok(match action {
            A::Concentric => {
                let (Some(m1), Some(m2)) = (round_center(sk, c[0]), round_center(sk, c[1])) else {
                    return Err(crate::msg!("sketch-concentric-kinds"));
                };
                let tied = sk.constraints().any(|(_, k)| {
                    k.enabled
                        && matches!(k.kind, K::Coincident(x, y)
                            if (x, y) == (m1, m2) || (x, y) == (m2, m1))
                });
                if m1 == m2 || tied {
                    return Err(crate::msg!("sketch-concentric-already"));
                }
                K::Coincident(m1, m2)
            }
            A::Collinear => K::Collinear(c[0], c[1]),
            A::Smooth => {
                let (pa, pb) = sk
                    .smooth_ends(c[0], self.pick_of(c[0]), c[1], self.pick_of(c[1]))
                    .ok_or_else(|| crate::msg!("sketch-err-smooth-end"))?;
                K::Smooth(c[0], pa, c[1], pb)
            }
            A::Fixed => {
                let xy = sk.point_xy(p[0]).map_err(err)?;
                K::Fixed(p[0], xy.x, xy.y)
            }
            A::Coincident => K::Coincident(p[0], p[1]),
            A::Horizontal | A::Vertical => {
                let lp = if p.len() == 2 {
                    LineOrPoints::Points(p[0], p[1])
                } else {
                    LineOrPoints::Line(c[0])
                };
                if action == A::Horizontal {
                    K::Horizontal(lp)
                } else {
                    K::Vertical(lp)
                }
            }
            A::Distance => {
                if p.len() == 2 {
                    K::Distance(p[0], p[1], 0.0)
                } else {
                    let Some(CurveKind::Line { a, b }) = sk.curve(c[0]).map(|d| d.kind.clone())
                    else {
                        return Err(crate::msg!("sketch-length-lines-only"));
                    };
                    K::Distance(a, b, 0.0)
                }
            }
            A::Parallel => K::Parallel(c[0], c[1]),
            A::Perpendicular => K::Perpendicular(c[0], c[1]),
            A::Tangent => K::Tangent(c[0], c[1]),
            A::Equal => K::Equal(c[0], c[1]),
            A::Angle => K::Angle(c[0], c[1], 0.0),
            A::Radius => K::Radius(c[0], 0.0),
            A::Diameter => K::Diameter(c[0], 0.0),
            A::PointOnCurve => K::PointOnCurve(p[0], c[0]),
            A::Midpoint => K::Midpoint(p[0], c[0]),
            A::DistancePointLine => K::DistancePointLine(p[0], c[0], 0.0),
            A::Symmetric => K::Symmetric(p[0], p[1], c[0]),
        })
    }

    /// Legt die Abhängigkeit `action` für die Auswahl an und löst. Maße ohne `value` erhalten
    /// den aktuellen Wert. Widersprüchliche oder unlösbare Abhängigkeiten werden verworfen
    /// (Skizze unverändert), überzählige Maße werden nur anzeigend.
    pub fn apply(
        &mut self,
        sk: &mut Sketch,
        action: ConstraintAction,
        value: Option<f64>,
    ) -> Result<ConstraintId, String> {
        // Sicherheitsnetz: Klicklagen aus einer anders geänderten Auswahl zählen nicht.
        self.forget_stale_picks();
        self.atomic(sk, |se, sk, prior| {
            let mut kind = se.build(sk, action)?;
            if action.is_dimension() {
                let v = match value {
                    Some(v) => v,
                    None => sk
                        .measure(&kind)
                        .ok_or_else(|| crate::msg!("sketch-cannot-measure"))?,
                };
                kind = checked_value(sk, &kind, v)?;
                // Ein getippter Winkel gilt nur bis auf 180° (wie bei `set_dimension_value`).
                if value.is_some()
                    && matches!(kind, ConstraintKind::Angle(..))
                    && !angle_value_ok(sk.measure(&kind), v)
                {
                    return Err(crate::msg!("sketch-angle-range"));
                }
            }
            // Glatt an getrennten Enden: erst die Enden zusammen (im selben Schritt).
            if let ConstraintKind::Smooth(_, pa, _, pb) = kind
                && pa != pb
                && !sk.constraints().any(|(_, k)| {
                    k.enabled
                        && matches!(k.kind, ConstraintKind::Coincident(x, y)
                            if (x, y) == (pa, pb) || (x, y) == (pb, pa))
                })
            {
                sk.add_constraint(ConstraintKind::Coincident(pa, pb))
                    .map_err(err)?;
            }
            se.add_checked(sk, prior, kind, action.label_msg())
        })
    }

    /// Legt `kind` an und löst, mit den Wächtern von [`apply`](Self::apply): Widerspruch oder
    /// nicht lösbar ⇒ Fehler (`label` nennt die Abhängigkeit), Falt-Wächter für Längen,
    /// überzähliges Maß ⇒ anzeigend. Läuft in [`atomic`](Self::atomic); `prior` ist der Stand davor.
    pub(crate) fn add_checked(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        kind: ConstraintKind,
        label: Msg,
    ) -> Result<ConstraintId, Msg> {
        let se = self;
        let dimension = kind.is_dimension();
        let on_arc = smooth_on_arc(sk, &kind);
        let old = sk.measure(&kind);
        let value_of = kind.length_value();
        let id = sk.add_constraint(kind).map_err(err)?;
        // Ein Maß nur an Exemplaren eines Musters legt die Skizze anzeigend an (es hätte sonst
        // keine Wirkung, Schlusskorrektur I1); sein Wert ist der gemessene.
        let driven = sk.constraint(id).is_some_and(|c| c.driven);
        // Der Stand vor dem Neulegen: Davon gehen Wächter und Schritte aus.
        let unsolved = sk.clone();
        if let Some(v) = value_of.filter(|_| !driven) {
            sk.prepare_dimension_change(id, v);
        }
        let before = se.last_report.take();
        // Freiheitsgrade davor: Fällt einer weg, war die neue Abhängigkeit nur teilweise
        // überzählig (etwa Kollinear an zwei waagerechten Linien).
        let report = sk.solve();
        se.last_report = Some(report.clone());
        let direct = match check_change(&report, before.as_ref(), prior, Some(id)) {
            Some(SolveStatus::Conflicting) => {
                Err(crate::msg!("sketch-apply-conflict", action = label.clone()))
            }
            Some(_) => Err(crate::msg!("sketch-apply-unsolved", action = label.clone())),
            None => Ok(()),
        };
        se.guard_length(sk, id, old, &unsolved, direct)?;
        let after = se.last_report.as_ref().unwrap_or(&report);
        if after.redundant.contains(&id) {
            if dimension {
                se.make_driven(sk, id)?;
            } else if after.dof < dof_of(prior) {
                se.message = Some(crate::msg!("sketch-redundant-partly"));
            } else {
                se.message = Some(crate::msg!("sketch-redundant"));
            }
        } else if driven {
            se.message = Some(crate::msg!("sketch-became-driven"));
        } else if on_arc {
            se.message = Some(crate::msg!("sketch-smooth-arc-g1"));
        }
        se.warn_if_unsolved();
        Ok(id)
    }

    /// Macht das überzählige Maß `id` anzeigend (es zeigt den aktuellen Wert) und sagt es.
    pub(crate) fn make_driven(&mut self, sk: &mut Sketch, id: ConstraintId) -> Result<(), Msg> {
        sk.set_driven(id, true).map_err(err)?;
        if let Some(v) = sk.constraint(id).and_then(|c| sk.measure(&c.kind)) {
            sk.set_constraint_value(id, v).map_err(err)?;
        }
        self.last_report = Some(sk.solve());
        self.message = Some(crate::msg!("sketch-became-driven"));
        Ok(())
    }

    /// Aktueller Wert des Maßes `action` für die Auswahl (Vorgabe des Zahlenfelds); `None`, wenn
    /// das kein Maß ist oder es nicht zur Auswahl passt.
    pub fn measure_action(&self, sk: &Sketch, action: ConstraintAction) -> Option<f64> {
        if !action.is_dimension() {
            return None;
        }
        let kind = self.build(sk, action).ok()?;
        sk.measure(&kind)
    }

    /// Ändert den Wert des Maßes `id` und löst. Ungültige Werte und Werte, die die Skizze
    /// widersprüchlich oder unlösbar machen, werden abgelehnt (Skizze unverändert).
    pub fn set_dimension_value(
        &mut self,
        sk: &mut Sketch,
        id: ConstraintId,
        value: f64,
    ) -> Result<(), String> {
        self.atomic(sk, |se, sk, prior| {
            let kind = sk
                .constraint(id)
                .map(|c| c.kind.clone())
                .ok_or_else(|| crate::msg!("sketch-constraint-missing"))?;
            checked_value(sk, &kind, value)?;
            // Ein Winkelmaß gilt nur bis 180° im Betrag: Darüber zeigte es einen Wert, den die Form nicht
            // hat. Unverändert bestätigen geht immer (ein getippter Öffnungswinkel darf größer sein).
            if matches!(kind, ConstraintKind::Angle(_, _, v) if v != value)
                && !angle_value_ok(sk.measure(&kind), value)
            {
                return Err(crate::msg!("sketch-angle-range"));
            }
            let old = sk.measure(&kind);
            let users = sk.constraints().count();
            sk.set_constraint_value(id, value).map_err(err)?;
            // Eine kleinere Anzahl eines Musters nimmt die Abhängigkeiten der wegfallenden
            // Exemplare mit; die Meldung nennt ihre Zahl (Skizzen 5).
            let dropped = users.saturating_sub(sk.constraints().count());
            // Der Stand vor dem Neulegen: Davon gehen Wächter und Schritte aus.
            let unsolved = sk.clone();
            sk.prepare_dimension_change(id, value);
            // Versatzmaß: Die Vorlage bleibt stehen, wenn das geht. Gelingt es nicht, ist nichts
            // passiert und es läuft das normale Lösen. So oder so prüft der Wächter gegen den Stand
            // davor, das Halten umgeht also keine Prüfung auf Umklappen.
            let hold = sk.offset_hold_points(id);
            // Was der Plan zum Wert sagt, nur für den Text einer Ablehnung und, bei „geht nicht“,
            // statt der teuren Schritte.
            let problem = hold
                .as_ref()
                .and_then(|_| unsolved.offset_problem_at(id, value));
            if let Some(hold) = &hold {
                match old.filter(|_| problem.is_none()) {
                    Some(old) => sk.solve_holding_in_steps(id, hold, old, value),
                    None => sk.solve_holding(hold),
                };
            }
            let direct = se.solve_checked(sk, prior).map(|_| ());
            let vanished = problem == Some(OffsetProblem::Vanished);
            if let Err(e) = se.guard_length(sk, id, old, &unsolved, direct) {
                return Err(if vanished {
                    crate::msg!("sketch-offset-vanished")
                } else {
                    e
                });
            }
            // Geht es nur, indem die Vorlage nachgibt (ein freier Bogen wüchse mit), passt der Wert
            // nicht: Sie bleibt stehen, das ist der Sinn dieses Maßes.
            let tol = sk.tolerance().linear;
            let moved = |p: &PointId| match (unsolved.point_xy(*p), sk.point_xy(*p)) {
                (Ok(a), Ok(b)) => a.distance(b) > tol,
                _ => true,
            };
            if hold.is_some_and(|h| h.iter().any(moved)) {
                return Err(if vanished {
                    crate::msg!("sketch-offset-vanished")
                } else {
                    crate::msg!("sketch-change-conflict")
                });
            }
            if dropped > 0 {
                se.add_note(crate::msg!("sketch-pattern-dropped", n = dropped));
            }
            Ok(())
        })
    }

    /// Falt-Wächter für das Längenmaß `id` (Skizzen 3): `direct` ist das Ergebnis des direkten
    /// Lösens, `unsolved` der Stand davor (das Maß hat schon seinen neuen Wert, ein tangentialer
    /// Bogen aber noch seine alte Lage, siehe [`Sketch::prepare_dimension_change`]). Ist es gescheitert
    /// oder hat die Skizze umgeklappt, geht es von dort ab dem alten Wert `old` in Schritten
    /// ([`Sketch::solve_in_steps`]): Ein großer Sprung lässt den Löser scheitern, in kleinen
    /// Schritten bleibt die Lage. Kommt es nicht
    /// ans Ziel, bleibt es beim Fehler aus `direct` (ein echter Widerspruch), sonst
    /// `sketch-dimension-folds`. Andere Maße bleiben unberührt.
    pub(crate) fn guard_length(
        &mut self,
        sk: &mut Sketch,
        id: ConstraintId,
        old: Option<f64>,
        unsolved: &Sketch,
        direct: Result<(), Msg>,
    ) -> Result<(), Msg> {
        let Some(new) = sk.constraint(id).and_then(|c| c.kind.length_value()) else {
            // Ein Winkelmaß hat keinen Weg in Schritten; über die Ecke einer Fase geht es trotzdem nicht.
            let solved = self.last_report.as_ref().is_some_and(status_ok);
            if direct.is_ok() && solved && unsolved.chamfer_left_its_side(sk) {
                return Err(crate::msg!("sketch-dimension-folds"));
            }
            return direct;
        };
        // Ein Radius darf einen tangentialen Bogen auch nicht über 180° ausbeulen (Skizzen 4).
        let radius = sk.constraint(id).is_some_and(|c| c.kind.is_radius());
        let folds = |trial: &Sketch| {
            folded_over(unsolved, trial)
                || unsolved.chamfer_left_its_side(trial)
                || (radius && sweep_crossed_half_turn(unsolved, trial))
        };
        let solved = self.last_report.as_ref().is_some_and(status_ok);
        if direct.is_ok() && !(solved && folds(sk)) {
            return Ok(());
        }
        let stuck = direct.and(Err(crate::msg!("sketch-dimension-folds")));
        let Some(old) = old.filter(|o| *o != new) else {
            return stuck;
        };
        let accept = |trial: &Sketch, report: &SolveReport| status_ok(report) && !folds(trial);
        let Some((done, report)) = unsolved.solve_in_steps(&[(id, old, new)], accept) else {
            return stuck;
        };
        *sk = done;
        self.last_report = Some(report);
        Ok(())
    }

    /// Zahl oder Formel `text` für das bestehende Maß `id` des Entwurfs `sf`: auswerten mit den
    /// Parametern `scope` (Winkel in Grad, gespeichert in Bogenmaß), Wert setzen und lösen wie
    /// [`set_dimension_value`](Self::set_dimension_value), erst danach die Formel setzen bzw.
    /// bei einer Zahl nur die Formel dieses Maßes entfernen (Plan P2). Atomar: Bei einem Fehler
    /// bleiben Skizze und Formeln unverändert, `message` erklärt ihn. Ein anzeigendes Maß nimmt
    /// nur Zahlen.
    pub fn set_dimension_input(
        &mut self,
        sf: &mut SketchFeature,
        id: ConstraintId,
        text: &str,
        scope: &ParamScope,
    ) -> Result<(), String> {
        self.message = None;
        let read = match sf.sketch.constraint(id) {
            None => Err(crate::msg!("sketch-constraint-missing")),
            Some(c) if !c.kind.is_dimension() => Err(crate::msg!("sketch-not-dimension")),
            Some(c) => {
                let driven = c.driven;
                // Ein anzeigendes Maß nimmt nur Zahlen; in Zoll hat auch eine Zahl einen
                // Speichertext („1 in“), der entfällt.
                let formula = matches!(
                    crate::formula_field::classify(text),
                    Some(crate::formula_field::FieldInput::Formula(_))
                );
                dimension_input(text, constraint_unit(&c.kind), scope).and_then(|(v, f)| {
                    if driven && formula {
                        Err(crate::msg!("sketch-driven-no-formula"))
                    } else if driven {
                        Ok((v, None))
                    } else {
                        Ok((v, f))
                    }
                })
            }
        };
        let (value, formula) = match read {
            Ok(r) => r,
            Err(e) => {
                let text = e.text();
                self.message = Some(e);
                return Err(text);
            }
        };
        self.set_dimension_value(&mut sf.sketch, id, value)?;
        match formula {
            Some(f) => {
                sf.formulas.insert(id, f);
            }
            None => {
                sf.formulas.remove(&id);
            }
        }
        self.note_formula_problem(sf);
        Ok(())
    }

    /// Neues Maß `action` aus der Auswahl mit Zahl oder Formel `text` (wie
    /// [`apply`](Self::apply)); die Formel kommt erst nach dem erfolgreichen Lösen dazu. Wird
    /// das Maß anzeigend (überzählig), entfällt die Formel, und die Meldung von `apply` steht da.
    pub fn apply_dimension_input(
        &mut self,
        sf: &mut SketchFeature,
        action: ConstraintAction,
        text: &str,
        scope: &ParamScope,
    ) -> Result<ConstraintId, String> {
        self.message = None;
        let read = if action.is_dimension() {
            dimension_input(text, action.unit(), scope)
        } else {
            Err(crate::msg!("sketch-not-dimension"))
        };
        let (value, formula) = match read {
            Ok(r) => r,
            Err(e) => {
                let text = e.text();
                self.message = Some(e);
                return Err(text);
            }
        };
        let id = self.apply(&mut sf.sketch, action, Some(value))?;
        let driven = sf.sketch.constraint(id).is_none_or(|c| c.driven);
        match formula {
            Some(f) if !driven => {
                sf.formulas.insert(id, f);
            }
            _ => {
                sf.formulas.remove(&id);
            }
        }
        self.note_formula_problem(sf);
        Ok(id)
    }

    /// Neues Maß `action` aus der Auswahl mit dem Wert `value` (Skizzeneinheit, ohne Formel),
    /// sonst wie [`apply_dimension_input`](Self::apply_dimension_input). Für ein unverändert
    /// bestätigtes Maßfeld, dessen Text gerundet ist.
    pub fn apply_dimension_value(
        &mut self,
        sf: &mut SketchFeature,
        action: ConstraintAction,
        value: f64,
    ) -> Result<ConstraintId, String> {
        self.message = None;
        if !action.is_dimension() {
            let e = crate::msg!("sketch-not-dimension");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        }
        let id = self.apply(&mut sf.sketch, action, Some(value))?;
        sf.formulas.remove(&id);
        self.note_formula_problem(sf);
        Ok(id)
    }

    /// Lässt sich eine Skizze mit Formeln nach einer erlaubten Änderung nicht lösen (der Fehler
    /// bestand schon vorher), steht der Fehler, den sie beim Beenden bekommt (Plan P12), als
    /// Meldung da, sofern keine andere ansteht oder nur der allgemeine Hinweis.
    pub(crate) fn note_formula_problem(&mut self, sf: &SketchFeature) {
        if let Some(p) = formula_problem(sf, self.last_report.as_ref())
            && (self.message.is_none() || self.message == unsolved_hint(self.last_report.as_ref()))
        {
            self.message = Some(p);
        }
    }

    /// Schaltet das Maß `id` auf „nur anzeigend“ (`driven`) oder zurück auf bestimmend. Beim
    /// Zurückschalten übernimmt es den aktuellen Wert; wird die Skizze dadurch widersprüchlich,
    /// bleibt alles unverändert.
    pub fn set_driven(
        &mut self,
        sk: &mut Sketch,
        id: ConstraintId,
        driven: bool,
    ) -> Result<(), String> {
        self.atomic(sk, |se, sk, prior| {
            let kind = sk
                .constraint(id)
                .map(|c| c.kind.clone())
                .ok_or_else(|| crate::msg!("sketch-constraint-missing"))?;
            if !kind.is_dimension() {
                return Err(crate::msg!("sketch-only-dimension-driven"));
            }
            if let Some(v) = sk.measure(&kind) {
                sk.set_constraint_value(id, v).map_err(err)?;
            }
            sk.set_driven(id, driven).map_err(err)?;
            let before = se.last_report.take();
            let report = sk.solve();
            se.last_report = Some(report.clone());
            if let Some(problem) =
                check_change(&report, before.as_ref(), prior, (!driven).then_some(id))
            {
                return Err(match problem {
                    SolveStatus::Conflicting => crate::msg!("sketch-change-conflict"),
                    _ => crate::msg!("sketch-change-unsolved"),
                });
            }
            if !driven && report.redundant.contains(&id) {
                return Err(crate::msg!("sketch-stays-driven"));
            }
            se.warn_if_unsolved();
            Ok(())
        })
    }

    // ---- Bearbeiten ---------------------------------------------------------------------

    /// Löscht die Auswahl samt abhängiger Kurven und Abhängigkeiten. Punkte gelöschter (auch
    /// mitgelöschter) Kurven, die danach nichts mehr benutzt, werden mit entfernt; andere freie
    /// Punkte bleiben.
    pub fn delete_selection(&mut self, sk: &mut Sketch) {
        // Eine Einheit: Lehnt die Skizze einen Teil ab (das Abwählen eines Exemplars brächte ein
        // Muster in eine Schleife, ein Musterwert bleibt), bleibt alles, wie es war, und die
        // Meldung nennt den Grund. `atomic` meldet aufgelöste Muster.
        let _ = self.atomic(sk, |se, sk, _| {
            let mut orphans: Vec<PointId> = Vec::new();
            se.drop_curve_picks();
            // Abhängigkeiten, die mit abgewählten Exemplaren entfallen.
            let mut dropped = 0;
            for item in std::mem::take(&mut se.selection) {
                // Ein Exemplar eines Musters wird abgewählt, das Muster bleibt (Entscheidung 6 des
                // Plans Skizzen 5); seine anderen Kurven und Punkte sind danach fort.
                if let Some((id, at)) = crate::sketch_pattern::instance_of_item(sk, item) {
                    let users = sk.constraints().count();
                    sk.suppress_instance(id, at, true).map_err(err)?;
                    dropped += users.saturating_sub(sk.constraints().count());
                    continue;
                }
                match item {
                    Item::Constraint(c) => {
                        if let Err(e) = sk.remove_constraint(c)
                            && matches!(
                                sk.constraint(c).map(|d| &d.kind),
                                Some(ConstraintKind::PatternValue(..))
                            )
                        {
                            return Err(err(e));
                        }
                    }
                    Item::Curve(c) => {
                        if let Some(d) = sk.curve(c) {
                            orphans.extend(d.kind.points());
                        }
                        let _ = sk.remove_curve(c);
                    }
                    Item::Point(p) => {
                        // Andere Punkte der mitgelöschten Kurven werden ebenso geprüft.
                        orphans.extend(
                            sk.curves()
                                .map(|(_, c)| c.kind.points())
                                .filter(|pts| pts.contains(&p))
                                .flatten(),
                        );
                        let _ = sk.remove_point(p);
                    }
                }
            }
            sk.prune_points(&orphans);
            se.pending.retain(|&p| sk.point_xy(p).is_ok());
            se.forget_removed_curves(sk);
            se.line_arc = se
                .line_arc
                .filter(|d| sk.point_xy(d.start).is_ok() && sk.curve(d.prev).is_some());
            if se.dragging.is_some_and(|p| sk.point_xy(p).is_err()) {
                se.dragging = None;
            }
            se.curve_drag = None;
            se.last_report = Some(sk.solve());
            if dropped > 0 {
                se.add_note(crate::msg!("sketch-pattern-instance-dropped", n = dropped));
            }
            Ok(())
        });
    }

    /// Lässt sich „Konstruktion umschalten“ anwenden (mindestens eine Kurve ausgewählt)? Gilt
    /// für den Knopf im Skizzenfeld und seine Taste.
    pub fn can_toggle_construction(&self) -> bool {
        self.selection.iter().any(|i| matches!(i, Item::Curve(_)))
    }

    /// „Konstruktion umschalten“ wie Knopf und Taste: nur, wenn
    /// [`can_toggle_construction`](Self::can_toggle_construction) gilt, sonst nichts.
    pub fn toggle_construction_if_possible(&mut self, sk: &mut Sketch) {
        if self.can_toggle_construction() {
            self.toggle_construction(sk);
        }
    }

    /// Schaltet bei den ausgewählten Kurven die Konstruktionseigenschaft um.
    pub fn toggle_construction(&mut self, sk: &mut Sketch) {
        for item in &self.selection {
            if let Item::Curve(c) = *item
                && let Some(construction) = sk.curve(c).map(|d| d.construction)
            {
                let _ = sk.set_construction(c, !construction);
            }
        }
    }

    /// Lässt sich „Mittellinie umschalten“ anwenden (mindestens eine Linie ausgewählt)? Gilt
    /// für den Knopf im Skizzenfeld und die Aktion.
    pub fn can_toggle_centerline(&self, sk: &Sketch) -> bool {
        self.selection.iter().any(|i| {
            matches!(i, Item::Curve(c)
                if sk.curve(*c).is_some_and(|d| matches!(d.kind, CurveKind::Line { .. })))
        })
    }

    /// „Mittellinie umschalten“ wie Knopf und Aktion: nur, wenn
    /// [`can_toggle_centerline`](Self::can_toggle_centerline) gilt. Jede ausgewählte Linie wird
    /// einzeln umgeschaltet (Mittellinie ⇄ normal), andere Kurven bleiben, wie sie sind.
    pub fn toggle_centerline_if_possible(&mut self, sk: &mut Sketch) {
        if !self.can_toggle_centerline(sk) {
            return;
        }
        for item in &self.selection {
            if let Item::Curve(c) = *item
                && let Some(on) = sk.curve(c).map(|d| d.centerline)
            {
                let _ = sk.set_centerline(c, !on);
            }
        }
    }

    /// Statuszeile: Freiheitsgrade, Widerspruch, überzählige Abhängigkeiten.
    pub fn status(&self, sk: &Sketch) -> (String, Severity) {
        let report = match &self.last_report {
            Some(r) => r.clone(),
            None => sk.clone().solve(),
        };
        match report.status {
            SolveStatus::Conflicting => (
                tr!("sketch-status-conflict", n = report.conflicting.len()),
                Severity::Error,
            ),
            SolveStatus::NotConverged => (tr!("sketch-status-unsolvable"), Severity::Error),
            SolveStatus::Solved => {
                let mut text = match report.dof {
                    0 => tr!("sketch-status-solved"),
                    1 => tr!("sketch-status-dof-one"),
                    n => tr!("sketch-status-dof", n = n),
                };
                if report.redundant.is_empty() {
                    (text, Severity::Info)
                } else {
                    text.push_str(&tr!("sketch-status-redundant", n = report.redundant.len()));
                    (text, Severity::Warning)
                }
            }
        }
    }

    fn selected_curves(&self) -> Vec<CurveId> {
        self.selection
            .iter()
            .filter_map(|i| match i {
                Item::Curve(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Spiegelt die ausgewählten Kurven an der Linie `axis`.
    pub fn mirror_selection(&mut self, sk: &mut Sketch, axis: CurveId) -> Result<(), String> {
        let curves: Vec<CurveId> = self
            .selected_curves()
            .into_iter()
            .filter(|&c| c != axis)
            .collect();
        self.atomic(sk, |se, sk, prior| {
            sk.add_mirrored(&curves, axis).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Lineares Muster der ausgewählten Kurven: `count` Kopien im Abstand `offset`.
    pub fn pattern_selection(
        &mut self,
        sk: &mut Sketch,
        offset: DVec2,
        count: usize,
    ) -> Result<(), String> {
        let curves = self.selected_curves();
        self.atomic(sk, |se, sk, prior| {
            sk.add_linear_pattern(&curves, offset, count).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Kopiert eine Körperkante als feste Konstruktionsgeometrie in die Skizze.
    pub fn project_edge(
        &mut self,
        sk: &mut Sketch,
        solid: &Solid,
        edge: Id<Edge>,
    ) -> Result<CurveId, String> {
        self.atomic(sk, |se, sk, prior| {
            let c = sk.add_projected_edge(solid, edge).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(c)
        })
    }
}

/// Prüft den Bericht nach einer Änderung gegen den Zustand davor. Liefert den Grund
/// (`Conflicting`/`NotConverged`), wenn die Änderung zurückgenommen werden muss:
/// - die Skizze ist nicht gelöst, war es aber vorher;
/// - die neue Abhängigkeit `new` gehört zu den widersprüchlichen;
/// - die Skizze war schon vorher kaputt, jetzt widersprechen sich aber Abhängigkeiten, die es
///   vorher nicht taten, oder sie wurde von widersprüchlich zu nicht lösbar.
///
/// Der Zustand davor ist `before` (der letzte Bericht der Sitzung), sonst wird `prior` gelöst.
///
/// Grenze: Der Bericht nennt bei nicht lösbaren Gruppen nur Abhängigkeiten mit zu großem
/// Residuum; eine neue nicht lösbare Gruppe neben einem schon bestehenden Widerspruch wird nur
/// erkannt, wenn sie dabei neue Abhängigkeiten nennt.
pub(crate) fn check_change(
    report: &SolveReport,
    before: Option<&SolveReport>,
    prior: &Sketch,
    new: Option<ConstraintId>,
) -> Option<SolveStatus> {
    if status_ok(report) {
        return None;
    }
    let bad = Some(report.status);
    if new.is_some_and(|id| report.conflicting.contains(&id)) {
        return bad;
    }
    let solved_prior;
    let before = match before {
        Some(b) => b,
        None => {
            solved_prior = prior.clone().solve();
            &solved_prior
        }
    };
    if status_ok(before) {
        return bad;
    }
    let grown = report
        .conflicting
        .iter()
        .any(|c| !before.conflicting.contains(c));
    let worse =
        before.status == SolveStatus::Conflicting && report.status == SolveStatus::NotConverged;
    (grown || worse).then_some(report.status)
}

/// Die Kette um `c`, wie sie der Versatz nimmt: Querlinien und Rollkreise eines früheren Versatzes
/// verbinden nicht, also bleibt nur die Kette, in der `c` selbst liegt. Ist `c` selbst eine solche
/// Hilfe, nur `c`.
fn offset_chain_of(sk: &Sketch, c: CurveId) -> Result<Vec<CurveId>, Msg> {
    let chains = sk
        .ordered_chains(&sk.connected_chain(c))
        .map_err(offset_problem_message)?;
    let own = chains
        .iter()
        .find(|chain| chain.items.iter().any(|item| item.0 == c));
    Ok(own.map_or_else(
        || vec![c],
        |chain| chain.items.iter().map(|item| item.0).collect(),
    ))
}

/// Die Kette einer Vorauswahl: Hilfen eines früheren Versatzes (Konstruktionskurven neben normalen
/// Kurven) zählen nicht, und von Ketten, die über solche Hilfen zusammenhängen (Vorlage und Kopie),
/// bleibt die erste, wie beim Klick. Getrennte Ketten bleiben. Verzweigt sich die Auswahl, bleibt sie
/// ganz; die Meldung kommt mit dem Klick.
fn offset_preselection(sk: &Sketch, curves: Vec<CurveId>) -> Vec<CurveId> {
    let is_helper = |c: &CurveId| sk.curve(*c).is_some_and(|d| d.construction);
    let curves: Vec<CurveId> = if curves.iter().all(is_helper) {
        curves
    } else {
        curves.into_iter().filter(|c| !is_helper(c)).collect()
    };
    let Ok(chains) = sk.ordered_chains(&curves) else {
        return curves;
    };
    let mut kept: Vec<CurveId> = Vec::new();
    let mut linked: Vec<CurveId> = Vec::new();
    for chain in &chains {
        if chain.items.iter().any(|item| linked.contains(&item.0)) {
            continue;
        }
        kept.extend(chain.items.iter().map(|item| item.0));
        if let Some(first) = chain.items.first() {
            linked.extend(sk.connected_chain(first.0));
        }
    }
    kept
}

/// Die Meldung zu einem Versatz, der nicht geht.
fn offset_problem_message(problem: OffsetProblem) -> Msg {
    match problem {
        OffsetProblem::Branching => crate::msg!("sketch-offset-branching"),
        OffsetProblem::Vanished => crate::msg!("sketch-offset-vanished"),
        OffsetProblem::NoJoin => crate::msg!("sketch-offset-no-join"),
        OffsetProblem::SelfCrossing => crate::msg!("sketch-offset-crossing"),
        OffsetProblem::Cusp => crate::msg!("sketch-offset-cusp"),
        OffsetProblem::Degenerate => crate::msg!("sketch-field-rejected"),
    }
}

/// Das Maß `kind` mit dem Wert `value`, geprüft nach der gemeinsamen Regel der Skizze
/// ([`Sketch::check_constraint_value`]). Nicht endliche Eingaben ergeben die Meldung des
/// Zahlenfelds.
pub fn with_checked_value(
    sk: &Sketch,
    kind: &ConstraintKind,
    value: f64,
) -> Result<ConstraintKind, String> {
    checked_value(sk, kind, value).map_err(|m| m.text())
}

/// Auswahl nach Art getrennt; `None`, wenn etwas fehlt oder keine Geometrie ist.
fn sort_items(sk: &Sketch, items: &[Item]) -> Option<Sorted> {
    let mut s = Sorted::default();
    for item in items {
        match *item {
            Item::Point(p) => {
                sk.point_xy(p).ok()?;
                s.points.push(p);
            }
            Item::Curve(c) => {
                match sk.curve(c)?.kind {
                    CurveKind::Line { .. } => s.lines.push(c),
                    CurveKind::Circle { .. } | CurveKind::Arc { .. } => s.rounds.push(c),
                    CurveKind::Ellipse { .. }
                    | CurveKind::EllipticArc { .. }
                    | CurveKind::Spline { .. } => s.others.push(c),
                }
                s.curves.push(c);
            }
            Item::Constraint(_) => return None,
        }
    }
    Some(s)
}

/// Abhängigkeiten, die zu den Objekten `items` passen.
fn actions_for(sk: &Sketch, items: &[Item]) -> Vec<ConstraintAction> {
    use ConstraintAction::*;
    let Some(s) = sort_items(sk, items) else {
        return Vec::new();
    };
    let mut found = match (
        s.points.len(),
        s.lines.len(),
        s.rounds.len(),
        s.others.len(),
    ) {
        (1, 0, 0, 0) => vec![Fixed],
        (2, 0, 0, 0) => vec![Coincident, Horizontal, Vertical, Distance],
        (0, 1, 0, 0) => vec![Horizontal, Vertical, Distance],
        (0, 2, 0, 0) => vec![Parallel, Perpendicular, Equal, Angle, Collinear],
        (0, 1, 1, 0) | (0, 1, 0, 1) => vec![Tangent],
        (0, 0, 2, 0) => vec![Tangent, Equal],
        (0, 0, 1, 0) => vec![Radius, Diameter],
        (1, 1, 0, 0) => vec![PointOnCurve, Midpoint, DistancePointLine],
        (1, 0, 1, 0) | (1, 0, 0, 1) => vec![PointOnCurve],
        (2, 1, 0, 0) => vec![Symmetric],
        _ => Vec::new(),
    };
    if let [a, b] = s.curves[..]
        && s.points.is_empty()
    {
        if round_center(sk, a).is_some() && round_center(sk, b).is_some() {
            found.push(Concentric);
        }
        if smooth_kinds_ok(sk, a, b) {
            found.push(Smooth);
        }
    }
    found
}

/// Wie weit Objekte für eine Abhängigkeit reichen (Abhängigkeitsbefehl, [`Tool::Constraint`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fit {
    /// Sie reichen: Die Abhängigkeit kann entstehen.
    Complete,
    /// Ein Anfang: Mit weiteren Bezügen kann es reichen (auch keine Objekte).
    Partial,
    /// Passt nicht, auch nicht mit weiteren Bezügen.
    No,
}

/// Formen der geometrischen Abhängigkeiten als Anzahl (Punkte, Linien, Kreise/Bögen,
/// Ellipsen/Splines), wie [`actions_for`] sie annimmt; Konzentrisch und Glatt prüfen die Arten
/// eigens ([`constraint_fit`]).
fn constraint_forms(action: ConstraintAction) -> &'static [[usize; 4]] {
    use ConstraintAction::*;
    match action {
        Coincident => &[[2, 0, 0, 0]],
        Horizontal | Vertical => &[[2, 0, 0, 0], [0, 1, 0, 0]],
        Parallel | Perpendicular | Collinear => &[[0, 2, 0, 0]],
        Equal => &[[0, 2, 0, 0], [0, 0, 2, 0]],
        Tangent => &[[0, 1, 1, 0], [0, 1, 0, 1], [0, 0, 2, 0]],
        Fixed => &[[1, 0, 0, 0]],
        Midpoint => &[[1, 1, 0, 0]],
        PointOnCurve => &[[1, 1, 0, 0], [1, 0, 1, 0], [1, 0, 0, 1]],
        Symmetric => &[[2, 1, 0, 0]],
        Concentric | Smooth | Distance | DistancePointLine | Radius | Diameter | Angle => &[],
    }
}

/// Reichen `items` für `action`, sind sie ein Anfang oder passen sie nicht? Maße passen nie (sie
/// haben keinen Befehl).
fn constraint_fit(sk: &Sketch, action: ConstraintAction, items: &[Item]) -> Fit {
    use ConstraintAction::*;
    if action.is_dimension() {
        return Fit::No;
    }
    if actions_for(sk, items).contains(&action) {
        return Fit::Complete;
    }
    let Some(s) = sort_items(sk, items) else {
        return Fit::No;
    };
    let partial = match action {
        Concentric => {
            s.points.is_empty()
                && s.curves.len() < 2
                && s.curves.iter().all(|&c| round_center(sk, c).is_some())
        }
        Smooth => {
            s.points.is_empty()
                && s.curves.len() < 2
                && s.curves.iter().all(|&c| smooth_partner(sk, c))
        }
        _ => {
            let have = [
                s.points.len(),
                s.lines.len(),
                s.rounds.len(),
                s.others.len(),
            ];
            constraint_forms(action)
                .iter()
                .any(|form| have.iter().zip(form).all(|(h, f)| h <= f) && have != *form)
        }
    };
    if partial { Fit::Partial } else { Fit::No }
}

/// Meldung, wenn `items` nicht zu `action` passen: Bei Konzentrisch und Glatt mit Kurven der
/// falschen Art sagt sie, welche gehen.
fn constraint_mismatch(sk: &Sketch, action: ConstraintAction, items: &[Item]) -> Msg {
    let curves_only = sort_items(sk, items).filter(|s| s.points.is_empty() && !s.curves.is_empty());
    let wrong_kind = |ok: &dyn Fn(CurveId) -> bool| {
        curves_only
            .as_ref()
            .is_some_and(|s| s.curves.len() == 2 || s.curves.iter().any(|&c| !ok(c)))
    };
    match action {
        ConstraintAction::Concentric if wrong_kind(&|c| round_center(sk, c).is_some()) => {
            crate::msg!("sketch-concentric-kinds")
        }
        ConstraintAction::Smooth if wrong_kind(&|c| smooth_partner(sk, c)) => {
            crate::msg!("sketch-smooth-kinds")
        }
        _ => crate::msg!("sketch-action-mismatch", action = action.label_msg()),
    }
}

/// Kann die Kurve `c` an einem Glatt teilhaben (Spline, Linie oder Bogen)?
fn smooth_partner(sk: &Sketch, c: CurveId) -> bool {
    matches!(
        sk.curve(c).map(|d| &d.kind),
        Some(CurveKind::Spline { .. } | CurveKind::Line { .. } | CurveKind::Arc { .. })
    )
}

/// Mitte eines Kreises, Bogens, einer Ellipse oder eines Ellipsenbogens (Konzentrisch).
pub(crate) fn round_center(sk: &Sketch, c: CurveId) -> Option<PointId> {
    match sk.curve(c)?.kind {
        CurveKind::Circle { center, .. }
        | CurveKind::Arc { center, .. }
        | CurveKind::Ellipse { center, .. }
        | CurveKind::EllipticArc { center, .. } => Some(center),
        CurveKind::Line { .. } | CurveKind::Spline { .. } => None,
    }
}

/// Geht Glatt zwischen `a` und `b`? Ein Spline und eine Linie, ein Bogen oder ein zweiter Spline.
fn smooth_kinds_ok(sk: &Sketch, a: CurveId, b: CurveId) -> bool {
    let kind = |c: CurveId| sk.curve(c).map(|d| &d.kind);
    let spline = |k: Option<&CurveKind>| matches!(k, Some(CurveKind::Spline { .. }));
    let partner = |k: Option<&CurveKind>| {
        matches!(
            k,
            Some(CurveKind::Spline { .. } | CurveKind::Line { .. } | CurveKind::Arc { .. })
        )
    };
    a != b && ((spline(kind(a)) && partner(kind(b))) || (spline(kind(b)) && partner(kind(a))))
}

/// Freiheitsgrade von `sk`, frisch gerechnet (nicht aus einem möglicherweise älteren Bericht).
fn dof_of(sk: &Sketch) -> usize {
    sk.clone().solve().dof
}

/// Glatt an einem Bogen: bis Teilprojekt 7 nur tangential (G1), das sagt die Meldung.
fn smooth_on_arc(sk: &Sketch, kind: &ConstraintKind) -> bool {
    let ConstraintKind::Smooth(a, _, b, _) = *kind else {
        return false;
    };
    [a, b]
        .into_iter()
        .any(|c| matches!(sk.curve(c).map(|d| &d.kind), Some(CurveKind::Arc { .. })))
}

/// Darf ein Winkelmaß den Wert `value` bekommen? Der Betrag ist höchstens 180°; ein negativer
/// Wert nur, wenn der Winkel jetzt schon negativ gemessen wird (`measured`, im Uhrzeigersinn).
pub(crate) fn angle_value_ok(measured: Option<f64>, value: f64) -> bool {
    value.abs() <= std::f64::consts::PI && (value >= 0.0 || measured.is_some_and(|m| m < 0.0))
}

/// Wie [`with_checked_value`], mit der Meldung als Grund (zum Speichern).
pub(crate) fn checked_value(
    sk: &Sketch,
    kind: &ConstraintKind,
    value: f64,
) -> Result<ConstraintKind, Msg> {
    if !value.is_finite() {
        return Err(crate::msg!("sketch-enter-number"));
    }
    let changed = kind
        .with_value(value)
        .ok_or_else(|| crate::msg!("sketch-not-dimension"))?;
    sk.check_constraint_value(&changed).map_err(err)?;
    slot_overall_gap_ok(sk, &changed)?;
    Ok(changed)
}

/// Kleinster Abstand der Breite eines Langlochs „Gesamt“ unter seiner Gesamtlänge, als Anteil der
/// Gesamtlänge. Darunter liegen die Mitten fast aufeinander, und `|a − b|` der Zeile
/// `SlotOverall` ist bei a = b nicht differenzierbar: Gemessen (dichte Folgen hin und zurück,
/// L 0,5…60) scheiterten Änderungen von Breite oder Gesamtlänge mit einer irreführenden Meldung
/// („Widerspruch“ oder „klappt um“) bis zu einem Abstand von 0,016 mm bei L 60 (0,03 %).
const SLOT_OVERALL_MIN_GAP: f64 = 1e-3;
/// Untergrenze des Abstands in mm für kleine Langlöcher: Dort scheiterte es bis etwa 0,0096 mm
/// (L 0,5), 0,0072 mm (L 1), 0,0067 mm (L 10), unabhängig von der Größe; doppelter Spielraum.
const SLOT_OVERALL_MIN_GAP_ABS: f64 = 0.02;

/// Kleinster erlaubter Abstand `L − W` beim Langloch „Gesamt“ mit Gesamtlänge `l` (mm):
/// `max(0,1 % · l, 0,02 mm)`.
#[doc(hidden)]
pub fn slot_overall_min_gap(l: f64) -> f64 {
    (SLOT_OVERALL_MIN_GAP * l).max(SLOT_OVERALL_MIN_GAP_ABS)
}

/// Lässt das Maß `kind` (neuer Wert) zwischen Gesamtlänge und Breite eines Langlochs „Gesamt“
/// genug Abstand ([`slot_overall_min_gap`])? Geprüft wird die Gesamtlänge gegen ein
/// bestimmendes Durchmesser- oder Radiusmaß ihrer Kappe, ohne ein solches Maß gegen die gemessene
/// Breite.
fn slot_overall_gap_ok(sk: &Sketch, kind: &ConstraintKind) -> Result<(), Msg> {
    use ConstraintKind as K;
    let width_of = |cap: CurveId| {
        sk.constraints()
            .filter(|(_, c)| !c.driven)
            .find_map(|(_, c)| match c.kind {
                K::Diameter(k, d) if k == cap => Some(d),
                K::Radius(k, r) if k == cap => Some(2.0 * r),
                _ => None,
            })
    };
    let lengths_of = |cap: CurveId, w: f64| -> Vec<(f64, f64)> {
        sk.constraints()
            .filter(|(_, c)| !c.driven)
            .filter_map(|(_, c)| match c.kind {
                K::SlotOverall(_, _, k, l) if k == cap => Some((l, w)),
                _ => None,
            })
            .collect()
    };
    let pairs: Vec<(f64, f64)> = match *kind {
        // Ohne Breitenmaß gilt die gemessene Breite: Sonst schrumpfte der Löser die freie Breite.
        K::SlotOverall(_, _, cap, l) => width_of(cap)
            .or_else(|| sk.radius(cap).map(|r| 2.0 * r))
            .map(|w| (l, w))
            .into_iter()
            .collect(),
        K::Diameter(cap, d) => lengths_of(cap, d),
        K::Radius(cap, r) => lengths_of(cap, 2.0 * r),
        _ => Vec::new(),
    };
    if pairs.iter().any(|&(l, w)| l - w < slot_overall_min_gap(l)) {
        return Err(crate::msg!("sketch-slot-overall-gap"));
    }
    Ok(())
}
