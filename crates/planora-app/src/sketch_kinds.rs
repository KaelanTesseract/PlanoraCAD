//! Die Arten der Zeichenwerkzeuge als Daten (Skizzen 3): je Art Werkzeug, Stufen samt Feldern,
//! Aktion, Name und Hinweis. Das Werkzeug bleibt [`Tool`]; die zuletzt gewählte Art je Form steht
//! in [`SketchSession::kinds`]. Die alten Vorgabearten laufen über ihre `click_*`-Funktionen, alle
//! anderen über eine einzige Stufenmaschine ([`SketchSession::click_kind`]): Sie sammelt Lagen und
//! legt die Form erst beim letzten Klick an, also ändert sie die Skizze bis dahin nicht.

use crate::keymap::Action;
use crate::msg::Msg;
use crate::sketch_cursor::DrawCursor;
use crate::sketch_input::FieldKind;
use crate::sketch_state::{SketchSession, Snap, Tool};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::model::{CurveKind, PointId, Sketch};
use planora_sketch::shapes::ShapePlan;

/// Schritte je Vollkreis für die Vorschau einer Form.
const PREVIEW_STEPS_PER_TURN: usize = 96;

/// Eine Art, eine Form zu zeichnen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Rect2P,
    Rect3P,
    RectCenter,
    CircleCenter,
    Circle2P,
    Circle3P,
    Circle2T,
    Circle3T,
    ArcCenter,
    Arc3P,
    ArcTangent,
    PolygonCircumscribed,
    PolygonInscribed,
    PolygonEdge,
    SlotCenterToCenter,
    SlotOverall,
    SlotCenterPoint,
    SlotThreePointArc,
    SlotCenterPointArc,
}

impl ToolKind {
    /// Alle Arten, je Form in der Reihenfolge ihrer Liste.
    pub const ALL: &'static [ToolKind] = &[
        ToolKind::Rect2P,
        ToolKind::Rect3P,
        ToolKind::RectCenter,
        ToolKind::CircleCenter,
        ToolKind::Circle2P,
        ToolKind::Circle3P,
        ToolKind::Circle2T,
        ToolKind::Circle3T,
        ToolKind::ArcCenter,
        ToolKind::Arc3P,
        ToolKind::ArcTangent,
        ToolKind::PolygonCircumscribed,
        ToolKind::PolygonInscribed,
        ToolKind::PolygonEdge,
        ToolKind::SlotCenterToCenter,
        ToolKind::SlotOverall,
        ToolKind::SlotCenterPoint,
        ToolKind::SlotThreePointArc,
        ToolKind::SlotCenterPointArc,
    ];

    /// Das Werkzeug, zu dem die Art gehört.
    pub fn tool(self) -> Tool {
        use ToolKind::*;
        match self {
            Rect2P | Rect3P | RectCenter => Tool::Rectangle,
            CircleCenter | Circle2P | Circle3P | Circle2T | Circle3T => Tool::Circle,
            ArcCenter | Arc3P | ArcTangent => Tool::Arc,
            PolygonCircumscribed | PolygonInscribed | PolygonEdge => Tool::Polygon,
            SlotCenterToCenter | SlotOverall | SlotCenterPoint | SlotThreePointArc
            | SlotCenterPointArc => Tool::Slot,
        }
    }

    /// Die Arten eines Werkzeugs (leer, wenn es nur einen Weg gibt).
    pub fn of(tool: Tool) -> &'static [ToolKind] {
        let all = ToolKind::ALL;
        match tool {
            Tool::Rectangle => &all[0..3],
            Tool::Circle => &all[3..8],
            Tool::Arc => &all[8..11],
            Tool::Polygon => &all[11..14],
            Tool::Slot => &all[14..19],
            _ => &[],
        }
    }

    /// Die alte Vorgabeart mit eigener `click_*`-Funktion?
    pub fn is_classic(self) -> bool {
        matches!(
            self,
            ToolKind::Rect2P | ToolKind::CircleCenter | ToolKind::ArcCenter
        )
    }

    /// Felder je Stufe (Index = Stufe, die letzte Stufe schließt ab, Stufe 0 hat nie welche).
    pub fn stages(self) -> &'static [&'static [FieldKind]] {
        use FieldKind as F;
        use ToolKind::*;
        match self {
            Rect2P | RectCenter => &[&[], &[F::Width, F::Height]],
            Rect3P => &[&[], &[F::Width, F::Angle], &[F::Depth]],
            CircleCenter | Circle2P => &[&[], &[F::Diameter]],
            Circle3P | Circle3T => &[&[], &[], &[]],
            Circle2T => &[&[], &[], &[F::Diameter]],
            ArcCenter => &[&[], &[F::Radius], &[F::Sweep]],
            Arc3P => &[&[], &[], &[F::Radius]],
            ArcTangent => &[&[], &[F::Radius, F::Sweep]],
            PolygonCircumscribed | PolygonInscribed => &[&[], &[F::Radius, F::Sides]],
            PolygonEdge => &[&[], &[F::Length, F::Sides], &[F::Sides]],
            SlotCenterToCenter | SlotOverall | SlotCenterPoint => &[&[], &[F::Length], &[F::Width]],
            SlotThreePointArc => &[&[], &[], &[F::Radius, F::Sweep], &[F::Width]],
            SlotCenterPointArc => &[&[], &[F::Radius], &[F::Sweep], &[F::Width]],
        }
    }

    /// Name in der aktuellen Sprache (der Name der Aktion).
    pub fn label(self) -> String {
        self.action().label()
    }

    /// Ein Satz zur Klickfolge in der aktuellen Sprache.
    pub fn hint(self) -> String {
        use ToolKind::*;
        match self {
            Rect2P => tr!("sketch-kind-rect-2p-hint"),
            Rect3P => tr!("sketch-kind-rect-3p-hint"),
            RectCenter => tr!("sketch-kind-rect-center-hint"),
            CircleCenter => tr!("sketch-kind-circle-center-hint"),
            Circle2P => tr!("sketch-kind-circle-2p-hint"),
            Circle3P => tr!("sketch-kind-circle-3p-hint"),
            Circle2T => tr!("sketch-kind-circle-2t-hint"),
            Circle3T => tr!("sketch-kind-circle-3t-hint"),
            ArcCenter => tr!("sketch-kind-arc-center-hint"),
            Arc3P => tr!("sketch-kind-arc-3p-hint"),
            ArcTangent => tr!("sketch-kind-arc-tangent-hint"),
            PolygonCircumscribed => tr!("sketch-kind-polygon-circumscribed-hint"),
            PolygonInscribed => tr!("sketch-kind-polygon-inscribed-hint"),
            PolygonEdge => tr!("sketch-kind-polygon-edge-hint"),
            SlotCenterToCenter => tr!("sketch-kind-slot-center-center-hint"),
            SlotOverall => tr!("sketch-kind-slot-overall-hint"),
            SlotCenterPoint => tr!("sketch-kind-slot-center-point-hint"),
            SlotThreePointArc => tr!("sketch-kind-slot-arc-3p-hint"),
            SlotCenterPointArc => tr!("sketch-kind-slot-arc-center-hint"),
        }
    }

    /// Die Aktion, die genau diese Art wählt.
    pub fn action(self) -> Action {
        use ToolKind::*;
        match self {
            Rect2P => Action::SketchRectangle,
            Rect3P => Action::SketchRect3P,
            RectCenter => Action::SketchRectCenter,
            CircleCenter => Action::SketchCircle,
            Circle2P => Action::SketchCircle2P,
            Circle3P => Action::SketchCircle3P,
            Circle2T => Action::SketchCircle2T,
            Circle3T => Action::SketchCircle3T,
            ArcCenter => Action::SketchArcCenter,
            Arc3P => Action::SketchArc3P,
            ArcTangent => Action::SketchArcTangent,
            PolygonCircumscribed => Action::SketchPolygonCircumscribed,
            PolygonInscribed => Action::SketchPolygonInscribed,
            PolygonEdge => Action::SketchPolygonEdge,
            SlotCenterToCenter => Action::SketchSlotCenterToCenter,
            SlotOverall => Action::SketchSlotOverall,
            SlotCenterPoint => Action::SketchSlotCenterPoint,
            SlotThreePointArc => Action::SketchSlotThreePointArc,
            SlotCenterPointArc => Action::SketchSlotCenterPointArc,
        }
    }

    /// Nimmt die Art beim Wählen von Kurven nur Linien?
    pub fn picks_lines_only(self) -> bool {
        self == ToolKind::Circle2T
    }
}

/// Je Form die zuletzt gewählte Art (gilt für die Sitzung).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolKinds {
    pub rectangle: ToolKind,
    pub circle: ToolKind,
    pub arc: ToolKind,
    pub polygon: ToolKind,
    pub slot: ToolKind,
}

impl Default for ToolKinds {
    fn default() -> ToolKinds {
        ToolKinds {
            rectangle: ToolKind::Rect2P,
            circle: ToolKind::CircleCenter,
            arc: ToolKind::ArcCenter,
            // Annahme: Vorgabe von Fusion.
            polygon: ToolKind::PolygonCircumscribed,
            slot: ToolKind::SlotCenterToCenter,
        }
    }
}

impl ToolKinds {
    fn remember(&mut self, kind: ToolKind) {
        match kind.tool() {
            Tool::Rectangle => self.rectangle = kind,
            Tool::Circle => self.circle = kind,
            Tool::Arc => self.arc = kind,
            Tool::Polygon => self.polygon = kind,
            Tool::Slot => self.slot = kind,
            _ => {}
        }
    }
}

impl SketchSession {
    /// Die zuletzt gewählte Art von `tool`; `None`, wenn es nur einen Weg gibt.
    pub fn kind_of(&self, tool: Tool) -> Option<ToolKind> {
        match tool {
            Tool::Rectangle => Some(self.kinds.rectangle),
            Tool::Circle => Some(self.kinds.circle),
            Tool::Arc => Some(self.kinds.arc),
            Tool::Polygon => Some(self.kinds.polygon),
            Tool::Slot => Some(self.kinds.slot),
            _ => None,
        }
    }

    /// Die aktive Art, wenn sie über die Stufenmaschine läuft (nicht klassisch).
    pub fn planned_kind(&self) -> Option<ToolKind> {
        self.kind_of(self.tool).filter(|k| !k.is_classic())
    }

    /// Wählt `kind` wie ihre Aktion: Ist sie schon gewählt, bleibt alles; sonst bricht das wie
    /// [`set_tool`](Self::set_tool) ab, auch innerhalb derselben Form.
    pub fn choose_kind(&mut self, sk: &mut Sketch, kind: ToolKind) {
        if self.tool == kind.tool() && self.kind_of(self.tool) == Some(kind) {
            return;
        }
        self.set_tool(sk, kind.tool());
        self.kinds.remember(kind);
    }

    /// Stufe der laufenden Form: gesetzte Lagen und gewählte Kurven.
    pub fn kind_stage(&self) -> usize {
        self.pending_pos.len() + self.pending_curves.len()
    }

    /// Wählt der nächste Klick eine Kurve statt einer Lage?
    pub fn picks_curves(&self) -> bool {
        match self.planned_kind() {
            Some(ToolKind::Circle3T) => true,
            Some(ToolKind::Circle2T) => self.kind_stage() < 2,
            _ => false,
        }
    }

    /// Hinweis zum aktiven Werkzeug in seiner aktiven Art.
    pub fn active_hint(&self) -> String {
        self.kind_of(self.tool)
            .map_or_else(|| crate::sketch_panel::tool_hint(self.tool), ToolKind::hint)
    }

    /// Klick in einer geplanten Art: Vor der letzten Stufe merkt er sich Lage und Fang (oder die
    /// gewählte Kurve), die letzte legt die Form an.
    pub(crate) fn click_kind(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let Some(kind) = self.planned_kind() else {
            return Ok(());
        };
        if self.kind_stage() + 1 >= kind.stages().len() {
            return self.finish_kind(sk, kind, cur, prior);
        }
        if self.picks_curves() {
            self.pick_curve(sk, kind, cur);
        } else {
            self.pending_pos.push(cur.at);
            self.pending_snaps.push(cur.remember(sk.tolerance().linear));
        }
        Ok(())
    }

    /// Nimmt die Kurve unter dem Zeiger; ohne passende Kurve passiert nichts.
    fn pick_curve(&mut self, sk: &Sketch, kind: ToolKind, cur: &DrawCursor) {
        let Snap::Curve(c) = cur.snap else {
            return;
        };
        let is_line = sk
            .curve(c)
            .is_some_and(|d| matches!(d.kind, CurveKind::Line { .. }));
        if (is_line || !kind.picks_lines_only()) && !self.pending_curves.contains(&c) {
            self.pending_curves.push(c);
        }
    }

    /// Legt die Form in einem Zug an, bindet und löst, setzt `created` und `edit_label` und ruft
    /// `clear_pending`. Die Arten füllen spätere Tasks; bis dahin lehnt es ab.
    fn finish_kind(
        &mut self,
        _sk: &mut Sketch,
        _kind: ToolKind,
        _cur: &DrawCursor,
        _prior: &Sketch,
    ) -> Result<(), Msg> {
        Err(crate::msg!("sketch-kind-pending"))
    }

    /// Der Plan der laufenden Form mit dem Zeiger an `at`; `None`, solange (oder wenn) daraus keine
    /// Form wird.
    pub fn kind_plan(&self, _sk: &Sketch, _at: DVec2) -> Option<ShapePlan> {
        None
    }

    /// Vorschau der laufenden Form, aus demselben Plan wie das Ergebnis.
    pub fn kind_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>> {
        Some(self.kind_plan(sk, at)?.outline(PREVIEW_STEPS_PER_TURN))
    }

    /// Lage mit den gesperrten Größen der laufenden Form; `None` ohne Sperre.
    pub(crate) fn kind_locked_position(&self, _sk: &Sketch, _cur: &DrawCursor) -> Option<DVec2> {
        None
    }

    /// Wert, den das Feld `kind` der laufenden Form vor dem ersten Zeichen zeigt.
    pub(crate) fn kind_field_value(&self, _sk: &Sketch, kind: FieldKind) -> Option<f64> {
        (kind == FieldKind::Sides).then(|| self.input.sides.unwrap_or(self.polygon_sides) as f64)
    }

    /// Bindet die eigenen Punkte `own` an die Fangziele `hits`: gebunden wird der Punkt an der Lage
    /// des Treffers ([`attach`](Self::attach)), ohne solchen Punkt nichts.
    pub(crate) fn bind_own(
        &mut self,
        sk: &mut Sketch,
        own: &[PointId],
        hits: &[DrawCursor],
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        for hit in hits {
            let found = own
                .iter()
                .copied()
                .find(|&p| sk.point_xy(p).is_ok_and(|q| q.distance(hit.at) <= tol));
            if found.is_some() {
                self.attach(sk, found, hit)?;
            }
        }
        Ok(())
    }
}
