//! Die Arten der Zeichenwerkzeuge als Daten (Skizzen 3): je Art Werkzeug, Stufen samt Feldern,
//! Aktion, Name und Hinweis. Das Werkzeug bleibt [`Tool`]; die zuletzt gewählte Art je Form steht
//! in [`SketchSession::kinds`]. Die alten Vorgabearten laufen über ihre `click_*`-Funktionen, alle
//! anderen über eine einzige Stufenmaschine ([`SketchSession::click_kind`]): Sie sammelt Lagen und
//! legt die Form erst beim letzten Klick an, also ändert sie die Skizze bis dahin nicht.

use crate::keymap::Action;
use crate::msg::Msg;
use crate::sketch_cursor::{DrawCursor, leaving_direction, tangent_start_curve};
use crate::sketch_input::{Created, FieldKind, locked_ray, toward_pointer};
use crate::sketch_state::{LineArcDrag, SketchSession, Snap, Tool, err};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::fillet::{ChamferKind, ChamferPlan, CurvePick, FilletPlan};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::{self, ArcPlan, ShapePlan, SlotIds, SlotStyle};
use planora_sketch::tangent_circles;

/// Schritte je Vollkreis für die Vorschau einer Form.
const PREVIEW_STEPS_PER_TURN: usize = 96;
/// Winkel der Fase „Abstand und Winkel“, solange keiner getippt ist (Annahme: Vorgabe von Fusion).
const CHAMFER_ANGLE_DEFAULT: f64 = std::f64::consts::FRAC_PI_4;

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
    /// Skizzenrundung.
    Fillet,
    /// Skizzenfase mit gleichem Abstand.
    ChamferEqual,
    /// Skizzenfase mit Abstand und Winkel.
    ChamferDistanceAngle,
    /// Skizzenfase mit zwei Abständen.
    ChamferTwoDistances,
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
        ToolKind::Fillet,
        ToolKind::ChamferEqual,
        ToolKind::ChamferDistanceAngle,
        ToolKind::ChamferTwoDistances,
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
            Fillet | ChamferEqual | ChamferDistanceAngle | ChamferTwoDistances => Tool::Fillet,
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
            Tool::Fillet => &all[19..23],
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
            Fillet => &[&[], &[], &[F::Radius]],
            ChamferEqual => &[&[], &[], &[F::Distance]],
            ChamferDistanceAngle => &[&[], &[], &[F::Distance, F::Angle]],
            ChamferTwoDistances => &[&[], &[], &[F::Distance, F::Distance2]],
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
            Fillet => tr!("sketch-kind-fillet-hint"),
            ChamferEqual => tr!("sketch-kind-chamfer-equal-hint"),
            ChamferDistanceAngle => tr!("sketch-kind-chamfer-distance-angle-hint"),
            ChamferTwoDistances => tr!("sketch-kind-chamfer-two-distances-hint"),
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
            Fillet => Action::SketchFillet,
            ChamferEqual => Action::SketchChamferEqual,
            ChamferDistanceAngle => Action::SketchChamferDistanceAngle,
            ChamferTwoDistances => Action::SketchChamferTwoDistances,
        }
    }

    /// Nimmt die Art beim Wählen von Kurven nur Linien?
    pub fn picks_lines_only(self) -> bool {
        self == ToolKind::Circle2T
    }

    /// Die Meldung, wenn aus den Lagen keine Form wird.
    fn refusal(self) -> Msg {
        use ToolKind::*;
        match self {
            Rect2P | Rect3P | RectCenter => crate::msg!("sketch-err-rect-size"),
            CircleCenter | Circle2P => crate::msg!("sketch-radius-positive"),
            Circle3P => crate::msg!("sketch-circle-collinear"),
            Circle2T | Circle3T => crate::msg!("sketch-tangent-no-solution"),
            ArcCenter | Arc3P | SlotThreePointArc | SlotCenterPointArc => {
                crate::msg!("sketch-arc-end-on-start")
            }
            ArcTangent => crate::msg!("sketch-arc-tangent-start"),
            PolygonCircumscribed | PolygonInscribed => crate::msg!("sketch-radius-positive"),
            PolygonEdge | SlotCenterToCenter | SlotOverall | SlotCenterPoint => {
                crate::msg!("sketch-err-line-zero")
            }
            Fillet => crate::msg!("sketch-fillet-too-big"),
            ChamferEqual | ChamferDistanceAngle | ChamferTwoDistances => {
                crate::msg!("sketch-chamfer-too-big")
            }
        }
    }

    /// Rundung oder eine der Fasen?
    fn is_fillet_or_chamfer(self) -> bool {
        self.tool() == Tool::Fillet
    }

    /// Ein gerades Langloch?
    fn is_straight_slot(self) -> bool {
        matches!(
            self,
            ToolKind::SlotCenterToCenter | ToolKind::SlotOverall | ToolKind::SlotCenterPoint
        )
    }

    /// Ein Bogenlangloch?
    fn is_arc_slot(self) -> bool {
        matches!(
            self,
            ToolKind::SlotThreePointArc | ToolKind::SlotCenterPointArc
        )
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
    pub fillet: ToolKind,
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
            fillet: ToolKind::Fillet,
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
            Tool::Fillet => self.fillet = kind,
            _ => {}
        }
    }
}

/// Taugt die Kurve für diese Art? Sonst die Meldung dazu.
fn check_pick(sk: &Sketch, kind: ToolKind, c: CurveId) -> Result<(), Msg> {
    let is_line = sk
        .curve(c)
        .is_some_and(|d| matches!(d.kind, CurveKind::Line { .. }));
    if kind.picks_lines_only() && !is_line {
        return Err(crate::msg!("sketch-tangent-lines-only"));
    }
    if !tangent_circles::is_tangent_candidate(sk, c) {
        return Err(crate::msg!("sketch-tangent-kinds"));
    }
    Ok(())
}

/// Taugt die Kurve für Rundung oder Fase? Die Fase nimmt nur Linien; die Rundung jede Kurve.
fn check_fillet_pick(sk: &Sketch, kind: ToolKind, c: CurveId) -> Result<(), Msg> {
    let is_line = sk
        .curve(c)
        .is_some_and(|d| matches!(d.kind, CurveKind::Line { .. }));
    if kind != ToolKind::Fillet && !is_line {
        return Err(crate::msg!("sketch-chamfer-lines"));
    }
    Ok(())
}

/// Lässt sich an den beiden Linien überhaupt eine Fase legen? Ein winziger Abstand scheitert nur
/// noch daran, dass das Ende an der Ecke festgehalten ist.
fn chamfer_possible(sk: &Sketch, a: CurvePick, b: CurvePick) -> bool {
    let probe = 2.0 * sk.tolerance().linear;
    sk.chamfer_plan(a, b, ChamferKind::Equal(probe)).is_some()
}

/// Passen die beiden Kurven zusammen? Dieselbe Kurve zweimal, parallele und tangentiale Kurven
/// haben keine Ecke; die Fase braucht außerdem ein freies Ende an der Ecke.
fn check_fillet_pair(sk: &Sketch, kind: ToolKind, a: CurvePick, b: CurvePick) -> Result<(), Msg> {
    if sk.fillet_corner(a, b).is_none() {
        return Err(crate::msg!("sketch-fillet-no-corner"));
    }
    if kind != ToolKind::Fillet && !chamfer_possible(sk, a, b) {
        return Err(crate::msg!("sketch-chamfer-fixed"));
    }
    Ok(())
}

/// Linie, Bogen oder Kreis (exakt gerechnet)?
fn is_exact_curve(sk: &Sketch, c: CurveId) -> bool {
    sk.curve(c).is_some_and(|d| {
        matches!(
            d.kind,
            CurveKind::Line { .. } | CurveKind::Arc { .. } | CurveKind::Circle { .. }
        )
    })
}

/// Ein Punkt auf `c` ein Viertel des Wegs von der Ecke zum anderen Ende: Beim Klick auf den
/// gemeinsamen Punkt steht er für die Klickstelle, und von zwei gemeinsamen Enden gilt das nähere.
fn spot_from_corner(sk: &Sketch, c: CurveId, corner: DVec2) -> Option<DVec2> {
    let line = sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN)?;
    let last = line.len().checked_sub(1).filter(|n| *n > 0)?;
    let share = if line[0].distance(corner) <= line[last].distance(corner) {
        0.25
    } else {
        0.75
    };
    let along = share * last as f64;
    let i = (along as usize).min(last - 1);
    let spot = line[i].lerp(line[i + 1], along - i as f64);
    sk.nearest_on_curve(c, spot)
}

/// Reicht der Radius `r` über die Sehne `start`–`end`? (Sie darf höchstens ein Durchmesser sein.)
fn radius_spans(start: DVec2, end: DVec2, r: f64, tol: f64) -> bool {
    r >= start.distance(end) / 2.0 - tol
}

/// Durchgangspunkt eines Bogens mit dem Radius `r` über der Sehne: auf der Mittelsenkrechten, die
/// Seite vom Zeiger. Liegt der Zeiger weiter als `r` von der Sehnenmitte, gilt der große Bogen.
fn through_at_radius(start: DVec2, end: DVec2, r: f64, pointer: DVec2, tol: f64) -> Option<DVec2> {
    if !radius_spans(start, end, r, tol) {
        return None;
    }
    let mid = (start + end) / 2.0;
    let half = start.distance(end) / 2.0;
    let normal = (end - start).try_normalize()?.perp();
    let side = if normal.dot(pointer - mid) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let long = r + (r * r - half * half).max(0.0).sqrt();
    let rise = if pointer.distance(mid) > r {
        long
    } else {
        half * half / long
    };
    Some(mid + normal * (side * rise))
}

/// Durchgangspunkt eines Bogens mit dem Öffnungswinkel `sweep` über der Sehne, die Seite vom Zeiger.
fn through_at_sweep(start: DVec2, end: DVec2, sweep: f64, pointer: DVec2) -> Option<DVec2> {
    let mid = (start + end) / 2.0;
    let normal = (end - start).try_normalize()?.perp();
    let side = if normal.dot(pointer - mid) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let half = sweep / 2.0;
    let r = start.distance(end) / 2.0 / half.sin();
    Some(mid + normal * (side * r * (1.0 - half.cos())))
}

/// Mittelbogen eines Bogenlanglochs aus den ersten beiden Lagen und der dritten: `SlotThreePointArc`
/// Start, Ende, Durchgang; `SlotCenterPointArc` Mitte, Start, Richtung des Endes (gegen den
/// Uhrzeigersinn).
fn slot_axis(kind: ToolKind, a: DVec2, b: DVec2, c: DVec2, tol: f64) -> Option<ArcPlan> {
    if kind == ToolKind::SlotThreePointArc {
        return shapes::arc_through(a, b, c, tol);
    }
    let end = a + (c - a).try_normalize()? * a.distance(b);
    (end.distance(b) > tol).then_some(ArcPlan {
        center: a,
        start: b,
        end,
    })
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
            Tool::Fillet => Some(self.kinds.fillet),
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
            Some(k) if k.is_fillet_or_chamfer() => self.kind_stage() < 2,
            _ => false,
        }
    }

    /// Seitenzahl des laufenden Polygons: die getippte, sonst die zuletzt abgeschlossene.
    pub fn current_sides(&self) -> usize {
        self.input.sides.unwrap_or(self.polygon_sides)
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
        if kind.is_fillet_or_chamfer() {
            return self.click_fillet(sk, kind, cur, prior);
        }
        if self.kind_stage() + 1 >= kind.stages().len() {
            return self.finish_kind(sk, kind, cur, prior);
        }
        if kind == ToolKind::ArcTangent {
            return self.begin_tangent_arc(sk, cur);
        }
        if self.picks_curves() {
            return self.pick_curve(sk, kind, cur);
        }
        let tol = sk.tolerance().linear;
        if self.pending_pos.iter().any(|p| p.distance(cur.at) <= tol) {
            return Err(kind.refusal());
        }
        // Die Stufe des Mittelbogens: Ohne Bogen wird es nie eine Form.
        if kind.is_arc_slot()
            && self.pending_pos.len() == 2
            && self.plan_with(sk, kind, cur.at, None).is_none()
        {
            return Err(self.finish_refusal(sk, kind));
        }
        self.pending_pos.push(cur.at);
        self.pending_snaps.push(cur.remember(tol));
        Ok(())
    }

    /// Klick von Rundung und Fase: Stufe 0 und 1 wählen Kurven samt Klickstelle (ein Punkt, an dem
    /// genau zwei Kurven enden, wählt in Stufe 0 beide), Stufe 2 schließt ab. Eine weitere Rundung
    /// derselben Sitzung schließt schon mit der zweiten Kurve ab. Klick ins Leere: nichts.
    fn click_fillet(
        &mut self,
        sk: &mut Sketch,
        kind: ToolKind,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if self.kind_stage() >= 2 {
            return self.finish_kind(sk, kind, cur, prior);
        }
        let corner_hit = (self.kind_stage() == 0)
            .then(|| sk.pick_point(cur.raw, self.pick_radius))
            .flatten();
        let picked: Vec<CurvePick> = if let Some(p) = corner_hit {
            let curves = sk.curves_at_point(p);
            let &[a, b] = curves.as_slice() else {
                return Err(crate::msg!("sketch-fillet-point-curves"));
            };
            let corner = sk.point_xy(p).map_err(err)?;
            let pick = |curve| {
                spot_from_corner(sk, curve, corner)
                    .map(|at| CurvePick { curve, at })
                    .ok_or_else(|| crate::msg!("sketch-fillet-no-corner"))
            };
            vec![pick(a)?, pick(b)?]
        } else {
            let Snap::Curve(curve) = cur.snap else {
                return Ok(());
            };
            vec![CurvePick { curve, at: cur.at }]
        };
        for pick in &picked {
            check_fillet_pick(sk, kind, pick.curve)?;
        }
        for pick in picked {
            if let (Some(&curve), Some(&at)) =
                (self.pending_curves.first(), self.pick_points.first())
            {
                check_fillet_pair(sk, kind, CurvePick { curve, at }, pick)?;
            }
            self.pending_curves.push(pick.curve);
            self.pick_points.push(pick.at);
        }
        if self.kind_stage() >= 2 && kind == ToolKind::Fillet && self.fillet_run.is_some() {
            return self.finish_kind(sk, kind, cur, prior);
        }
        Ok(())
    }

    /// Die beiden gewählten Kurven samt Klickstellen.
    fn curve_picks(&self) -> Option<[CurvePick; 2]> {
        match (self.pending_curves.as_slice(), self.pick_points.as_slice()) {
            (&[a, b], &[p, q]) => {
                Some([CurvePick { curve: a, at: p }, CurvePick { curve: b, at: q }])
            }
            _ => None,
        }
    }

    /// Abstand des Zeigers `at` zur Ecke der gewählten Kurven.
    fn pointer_to_corner(&self, sk: &Sketch, at: DVec2) -> Option<f64> {
        let [a, b] = self.curve_picks()?;
        Some(sk.fillet_corner(a, b)?.distance(at))
    }

    /// Länge aus dem Feld `field`, sonst der Abstand des Zeigers zur Ecke.
    fn length_at(&self, sk: &Sketch, field: FieldKind, at: DVec2) -> Option<f64> {
        self.input
            .locked(field)
            .or_else(|| self.pointer_to_corner(sk, at))
    }

    /// Plan der Rundung, für Vorschau und Klick: Im Lauf gilt der heutige Radius der ersten
    /// Rundung (`Equal` nimmt keinen anderen), sonst Feld oder Zeiger.
    fn fillet_plan_at(&self, sk: &Sketch, at: DVec2) -> Option<FilletPlan> {
        let [a, b] = self.curve_picks()?;
        let radius = match self.fillet_run {
            Some((arc, r)) => sk.radius(arc).unwrap_or(r),
            None => self.length_at(sk, FieldKind::Radius, at)?,
        };
        sk.fillet_plan(a, b, radius)
    }

    /// Plan der Fase, für Vorschau und Klick: Längen aus den Feldern, sonst vom Zeiger; der Winkel
    /// aus dem Feld, sonst die Vorgabe.
    fn chamfer_plan_at(&self, sk: &Sketch, kind: ToolKind, at: DVec2) -> Option<ChamferPlan> {
        use FieldKind as F;
        let [a, b] = self.curve_picks()?;
        let first = self.length_at(sk, F::Distance, at)?;
        let shape = match kind {
            ToolKind::ChamferEqual => ChamferKind::Equal(first),
            ToolKind::ChamferDistanceAngle => ChamferKind::DistanceAngle(
                first,
                self.input.locked(F::Angle).unwrap_or(CHAMFER_ANGLE_DEFAULT),
            ),
            ToolKind::ChamferTwoDistances => {
                ChamferKind::TwoDistances(first, self.length_at(sk, F::Distance2, at)?)
            }
            _ => return None,
        };
        sk.chamfer_plan(a, b, shape)
    }

    /// Legt Rundung oder Fase mit dem Plan der Vorschau an und löst. Fehler ändern nichts
    /// ([`atomic`](Self::atomic)), die Stufe bleibt.
    fn create_fillet(
        &mut self,
        sk: &mut Sketch,
        kind: ToolKind,
        at: DVec2,
        prior: &Sketch,
    ) -> Result<Created, Msg> {
        if kind == ToolKind::Fillet {
            let plan = self
                .fillet_plan_at(sk, at)
                .ok_or_else(|| self.fillet_refusal(sk))?;
            let ids = sk
                .add_fillet(&plan, self.fillet_run.map(|(arc, _)| arc))
                .map_err(err)?;
            self.solve_checked(sk, prior).map_err(|_| kind.refusal())?;
            self.fillet_run.get_or_insert((ids.arc, plan.radius));
            return Ok(Created::Fillet(ids));
        }
        let plan = self
            .chamfer_plan_at(sk, kind, at)
            .ok_or_else(|| self.chamfer_refusal(sk, kind))?;
        let ids = sk.add_chamfer(&plan).map_err(err)?;
        self.solve_checked(sk, prior).map_err(|_| kind.refusal())?;
        Ok(Created::Chamfer(ids))
    }

    /// Die Meldung, wenn zur letzten Stufe keine Rundung passt: Ellipse und Spline haben ihre eigene.
    fn fillet_refusal(&self, sk: &Sketch) -> Msg {
        if self.pending_curves.iter().all(|&c| is_exact_curve(sk, c)) {
            ToolKind::Fillet.refusal()
        } else {
            crate::msg!("sketch-fillet-kinds")
        }
    }

    /// Die Meldung, wenn zur letzten Stufe keine Fase passt.
    fn chamfer_refusal(&self, sk: &Sketch, kind: ToolKind) -> Msg {
        match self.curve_picks() {
            Some([a, b]) if !chamfer_possible(sk, a, b) => crate::msg!("sketch-chamfer-fixed"),
            _ => kind.refusal(),
        }
    }

    /// Nimmt die Kurve unter dem Zeiger: ohne Kurve oder bei einer schon gewählten passiert nichts,
    /// eine unpassende Art ergibt eine Meldung.
    fn pick_curve(&mut self, sk: &Sketch, kind: ToolKind, cur: &DrawCursor) -> Result<(), Msg> {
        let Snap::Curve(c) = cur.snap else {
            return Ok(());
        };
        check_pick(sk, kind, c)?;
        if !self.pending_curves.contains(&c) {
            self.pending_curves.push(c);
        }
        Ok(())
    }

    /// Erster Klick des tangentialen Bogens: Er braucht einen gefangenen Punkt, an dem eine Linie
    /// oder ein Bogen endet.
    fn begin_tangent_arc(&mut self, sk: &Sketch, cur: &DrawCursor) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let prev = cur
            .point(tol)
            .and_then(|p| tangent_start_curve(sk, p, None))
            .ok_or_else(|| ToolKind::ArcTangent.refusal())?;
        self.pending_pos.push(cur.at);
        self.pending_snaps.push(cur.remember(tol));
        self.auto_tangent = Some(prev);
        Ok(())
    }

    /// Anfang des laufenden tangentialen Bogens: Punkt und Vorgänger, Lage und Abgangsrichtung.
    fn tangent_start(&self, sk: &Sketch) -> Option<(PointId, CurveId, DVec2, DVec2)> {
        let prev = self.auto_tangent?;
        let hit = *self.pending_snaps.first()?;
        let start = DrawCursor::remembered(hit, sk).point(sk.tolerance().linear)?;
        let leaving = leaving_direction(sk, prev, start)?;
        Some((start, prev, sk.point_xy(start).ok()?, leaving))
    }

    /// Die Meldung, wenn der letzte Klick keinen Plan ergibt.
    fn finish_refusal(&self, sk: &Sketch, kind: ToolKind) -> Msg {
        match kind {
            ToolKind::Arc3P if self.input.locked(FieldKind::Radius).is_some() => {
                crate::msg!("sketch-arc-radius-chord")
            }
            ToolKind::Arc3P => crate::msg!("sketch-circle-collinear"),
            ToolKind::PolygonEdge => crate::msg!("sketch-polygon-edge-side"),
            ToolKind::ArcTangent if self.tangent_start(sk).is_some() => {
                crate::msg!("sketch-arc-tangent-straight")
            }
            // Die Stufe des Mittelbogens, siehe `click_kind`.
            ToolKind::SlotThreePointArc | ToolKind::SlotCenterPointArc
                if self.pending_pos.len() == 2 =>
            {
                match kind {
                    ToolKind::SlotCenterPointArc => kind.refusal(),
                    _ if self.input.locked(FieldKind::Radius).is_some() => {
                        crate::msg!("sketch-arc-radius-chord")
                    }
                    _ => crate::msg!("sketch-circle-collinear"),
                }
            }
            _ if kind.is_straight_slot() || kind.is_arc_slot() => {
                crate::msg!("sketch-slot-too-wide")
            }
            _ => kind.refusal(),
        }
    }

    /// Tangentialer Bogen ab `start`, das Ende an der Lage von `cur`: Mitte und Ende anlegen, bei
    /// getauschter Lage (der Bogen läuft gegen den Uhrzeigersinn) `start` als Ende, dann die Tangente
    /// an `prev`, der Fang am Ende, lösen. Setzt `created` und `edit_label`.
    pub(crate) fn create_tangent_arc(
        &mut self,
        sk: &mut Sketch,
        start: PointId,
        prev: CurveId,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<CurveId, Msg> {
        let straight = || crate::msg!("sketch-arc-tangent-straight");
        let from = sk.point_xy(start).map_err(err)?;
        let leaving = leaving_direction(sk, prev, start).ok_or_else(straight)?;
        let plan = shapes::tangent_arc(from, leaving, cur.at, sk.tolerance().linear)
            .ok_or_else(straight)?;
        let center = sk.add_point(plan.center.x, plan.center.y).map_err(err)?;
        let end = self.attach(sk, None, cur)?;
        let arc = if plan.start == from {
            sk.add_arc(center, start, end)
        } else {
            sk.add_arc(center, end, start)
        }
        .map_err(err)?;
        sk.add_constraint(ConstraintKind::Tangent(arc, prev))
            .map_err(err)?;
        self.solve_checked(sk, prior)?;
        let created = Created::Arc(arc);
        self.edit_label = Some(created.undo_label());
        self.created = Some(created);
        Ok(arc)
    }

    /// Drücken auf dem letzten Punkt des Linienzugs wird beim Ziehen ein tangentialer Bogen: Greift
    /// nur im Linienwerkzeug, wenn `at` im Fangradius des Punkts liegt und dort eine Kurve endet.
    pub fn begin_line_arc(&mut self, sk: &Sketch, at: DVec2, radius: f64) -> bool {
        if self.tool != Tool::Line || !at.is_finite() {
            return false;
        }
        let Some(&start) = self.pending.first() else {
            return false;
        };
        if sk.point_xy(start).map_or(true, |p| p.distance(at) > radius) {
            return false;
        }
        let Some(prev) = tangent_start_curve(sk, start, self.line_chain) else {
            return false;
        };
        self.line_arc = Some(LineArcDrag { start, prev });
        true
    }

    /// Loslassen: legt den Bogen wie der tangentiale Bogen an, der Linienzug läuft von dessen Ende
    /// weiter. Der Zug endet in jedem Fall; wird kein Bogen daraus, bleibt der Linienzug, wie er
    /// war, und `message` erklärt es.
    pub fn finish_line_arc(
        &mut self,
        sk: &mut Sketch,
        raw: DVec2,
        radius: f64,
    ) -> Result<(), String> {
        if self.line_arc.is_none() || !raw.is_finite() {
            self.line_arc = None;
            return Ok(());
        }
        self.cursor = Some(raw);
        self.created = None;
        // Noch während des Zugs auflösen: ohne Felder und ohne Richtung.
        let cur = self.resolve_cursor(sk, raw, radius);
        let Some(drag) = self.line_arc.take() else {
            return Ok(());
        };
        self.atomic(sk, |se, sk, prior| {
            let arc = se.create_tangent_arc(sk, drag.start, drag.prev, &cur, prior)?;
            let far_end = match sk.curve(arc).map(|d| &d.kind) {
                Some(&CurveKind::Arc { start, end, .. }) if start == drag.start => end,
                Some(&CurveKind::Arc { start, .. }) => start,
                _ => return Err(ToolKind::ArcTangent.refusal()),
            };
            se.pending = vec![far_end];
            se.line_chain = Some(arc);
            se.line_after_arc = Some(arc);
            Ok(())
        })
    }

    /// Vorschau des laufenden Zugs auf dem letzten Punkt: der tangentiale Bogen bis `at`.
    pub fn line_arc_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>> {
        let drag = self.line_arc?;
        let from = sk.point_xy(drag.start).ok()?;
        let leaving = leaving_direction(sk, drag.prev, drag.start)?;
        let arc = shapes::tangent_arc(from, leaving, at, sk.tolerance().linear)?;
        Some(ShapePlan::Arc(arc).outline(PREVIEW_STEPS_PER_TURN))
    }

    /// Legt die Form in einem Zug an, bindet und löst, setzt `created` und `edit_label` und ruft
    /// `clear_pending`. Wird aus den Lagen keine Form, bleibt alles bis auf die Meldung, wie es war.
    fn finish_kind(
        &mut self,
        sk: &mut Sketch,
        kind: ToolKind,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if kind.is_fillet_or_chamfer() {
            let created = self.create_fillet(sk, kind, cur.at, prior)?;
            self.modify_edits = (self.modify_edits.0 + 1, Some(self.tool));
            self.edit_label = Some(created.undo_label());
            self.created = Some(created);
            self.clear_pending();
            return Ok(());
        }
        // Beim letzten Klick von `3 tangential` zählt die Kurve unter dem Zeiger.
        let third = match (kind, cur.snap) {
            (ToolKind::Circle3T, Snap::Curve(c)) if !self.pending_curves.contains(&c) => {
                check_pick(sk, kind, c)?;
                Some(c)
            }
            (ToolKind::Circle3T, _) => return Ok(()),
            _ => None,
        };
        let at = if third.is_some() { cur.raw } else { cur.at };
        let plan = self
            .plan_with(sk, kind, at, third)
            .ok_or_else(|| self.finish_refusal(sk, kind))?;
        let mut hits: Vec<DrawCursor> = self
            .pending_snaps
            .iter()
            .map(|&hit| DrawCursor::remembered(hit, sk))
            .collect();
        hits.push(cur.clone());
        let created = match (kind, plan) {
            (ToolKind::Rect3P, ShapePlan::Quad(corners)) => {
                let rect = sk.add_rect_three_point(corners).map_err(err)?;
                self.bind_own(sk, &rect.points, &hits)?;
                self.solve_checked(sk, prior)?;
                Created::Rectangle(rect)
            }
            (ToolKind::RectCenter, ShapePlan::Quad(_)) => {
                let ids = sk.add_rect_center(self.pending_pos[0], at).map_err(err)?;
                let mut own = vec![ids.center];
                own.extend(ids.rect.points);
                self.bind_own(sk, &own, &hits)?;
                self.solve_checked(sk, prior)?;
                Created::Rectangle(ids.rect)
            }
            (ToolKind::Circle2T | ToolKind::Circle3T, ShapePlan::Circle(m, r)) => {
                let mut curves = self.pending_curves.clone();
                curves.extend(third);
                Created::Circle(self.add_tangent_circle(sk, m, r, &curves, prior)?)
            }
            (_, ShapePlan::Circle(m, r)) => {
                let center = sk.add_point(m.x, m.y).map_err(err)?;
                let circle = sk.add_circle(center, r).map_err(err)?;
                for hit in &hits {
                    self.bind_rim(sk, circle, hit)?;
                }
                self.solve_checked(sk, prior)?;
                Created::Circle(circle)
            }
            (ToolKind::Arc3P, ShapePlan::Arc(arc)) => {
                let center = sk.add_point(arc.center.x, arc.center.y).map_err(err)?;
                let first = self.attach(sk, None, &hits[0])?;
                let second = self.attach(sk, None, &hits[1])?;
                let curve = if arc.start == self.pending_pos[0] {
                    sk.add_arc(center, first, second)
                } else {
                    sk.add_arc(center, second, first)
                }
                .map_err(err)?;
                self.bind_rim(sk, curve, &hits[2])?;
                self.solve_checked(sk, prior)?;
                Created::Arc(curve)
            }
            (ToolKind::ArcTangent, ShapePlan::Arc(_)) => {
                let (start, prev, ..) = self.tangent_start(sk).ok_or_else(|| kind.refusal())?;
                Created::Arc(self.create_tangent_arc(sk, start, prev, cur, prior)?)
            }
            (
                ToolKind::PolygonInscribed | ToolKind::PolygonCircumscribed | ToolKind::PolygonEdge,
                ShapePlan::Polygon(poly),
            ) => {
                let ids = sk.add_polygon(&poly).map_err(err)?;
                let tol = sk.tolerance().linear;
                let circumscribed = kind == ToolKind::PolygonCircumscribed;
                let own = [ids.center, ids.points[0], ids.points[1]];
                // Umbeschrieben liegt der letzte Klick an der Seitenmitte, nicht an einem eigenen Punkt.
                let bound = if circumscribed {
                    &hits[..hits.len() - 1]
                } else {
                    &hits[..]
                };
                self.bind_own(sk, &own, bound)?;
                if let Some(q) = cur
                    .point(tol)
                    .filter(|q| circumscribed && *q != ids.center && !ids.points.contains(q))
                {
                    sk.add_constraint(ConstraintKind::Midpoint(q, ids.lines[0]))
                        .map_err(err)?;
                }
                self.solve_checked(sk, prior)?;
                self.polygon_sides = self.current_sides();
                Created::Polygon {
                    ids,
                    inscribed: kind == ToolKind::PolygonInscribed,
                }
            }
            (_, ShapePlan::Slot(plan)) => {
                let style = if kind == ToolKind::SlotCenterPoint {
                    SlotStyle::CenterPoint
                } else {
                    SlotStyle::CenterToCenter
                };
                let ids = sk.add_slot(&plan, style).map_err(err)?;
                self.bind_slot(sk, kind, &ids, &hits[..hits.len() - 1])?;
                self.solve_checked(sk, prior)?;
                Created::Slot(ids)
            }
            (_, ShapePlan::ArcSlot(plan)) => {
                let ids = sk.add_arc_slot(&plan).map_err(err)?;
                let own = [ids.center, ids.ends[0], ids.ends[1]];
                self.bind_own(sk, &own, &hits[..hits.len() - 1])?;
                self.solve_checked(sk, prior)?;
                Created::ArcSlot(ids)
            }
            _ => return Err(kind.refusal()),
        };
        self.edit_label = Some(created.undo_label());
        self.created = Some(created);
        self.clear_pending();
        Ok(())
    }

    /// Bindet die ersten Klicks eines geraden Langlochs (die Breite bindet nie). „Gesamt“ hat an den
    /// äußeren Enden keine eigenen Punkte (Ruling Task 2): Der gefangene Punkt liegt auf dem Ende.
    fn bind_slot(
        &mut self,
        sk: &mut Sketch,
        kind: ToolKind,
        ids: &SlotIds,
        hits: &[DrawCursor],
    ) -> Result<(), Msg> {
        if kind != ToolKind::SlotOverall {
            let mut own = ids.centers.to_vec();
            own.extend(ids.mid);
            return self.bind_own(sk, &own, hits);
        }
        let tol = sk.tolerance().linear;
        for (hit, cap) in hits.iter().zip(ids.caps) {
            if !matches!(hit.remember(tol).snap, Snap::None | Snap::Grid) {
                let q = self.attach(sk, None, hit)?;
                sk.add_constraint(ConstraintKind::PointOnCurve(q, cap))
                    .map_err(err)?;
            }
        }
        Ok(())
    }

    /// Kreis exakt an der gerechneten Lage, dann die Tangenten. Wandert seine Mitte beim Lösen, wäre
    /// es eine andere Lösung als die Vorschau: abgelehnt.
    fn add_tangent_circle(
        &mut self,
        sk: &mut Sketch,
        m: DVec2,
        r: f64,
        curves: &[CurveId],
        prior: &Sketch,
    ) -> Result<CurveId, Msg> {
        let center = sk.add_point(m.x, m.y).map_err(err)?;
        let circle = sk.add_circle(center, r).map_err(err)?;
        for &c in curves {
            sk.add_constraint(ConstraintKind::Tangent(c, circle))
                .map_err(err)?;
        }
        let refused = || ToolKind::Circle3T.refusal();
        self.solve_checked(sk, prior).map_err(|_| refused())?;
        let moved = sk.point_xy(center).map_err(err)?.distance(m);
        if moved > 10.0 * sk.tolerance().linear {
            return Err(refused());
        }
        Ok(circle)
    }

    /// Der Plan der laufenden Form mit dem Zeiger an `at`; `None`, solange (oder wenn) daraus keine
    /// Form wird. Beim letzten Klick von `3 tangential` ist `at` die Rohlage, die dritte Kurve steht
    /// im Zeiger dieses Bilds ([`update_cursor`](Self::update_cursor)). Ein Bogenlangloch zeigt in der
    /// Stufe seines Mittelbogens diesen Bogen.
    pub fn kind_plan(&self, sk: &Sketch, at: DVec2) -> Option<ShapePlan> {
        let third = self.draw_cursor.as_ref().and_then(|c| match c.snap {
            Snap::Curve(curve) => Some(curve),
            _ => None,
        });
        self.plan_with(sk, self.planned_kind()?, at, third)
    }

    /// Dieselbe Rechnung für Vorschau und Klick.
    fn plan_with(
        &self,
        sk: &Sketch,
        kind: ToolKind,
        at: DVec2,
        third: Option<CurveId>,
    ) -> Option<ShapePlan> {
        let tol = sk.tolerance().linear;
        let circle = |(m, r)| ShapePlan::Circle(m, r);
        match (
            kind,
            self.pending_pos.as_slice(),
            self.pending_curves.as_slice(),
        ) {
            (ToolKind::Fillet, _, _) => self.fillet_plan_at(sk, at).map(|p| ShapePlan::Arc(p.arc)),
            (ToolKind::Rect3P, &[a, b], _) => {
                shapes::rect_three_point(a, b, at, tol).map(ShapePlan::Quad)
            }
            (ToolKind::RectCenter, &[center], _) => {
                shapes::rect_center(center, at, tol).map(ShapePlan::Quad)
            }
            (ToolKind::Circle2P, &[a], _) => {
                let r = a.distance(at) / 2.0;
                (r > tol && r.is_finite()).then(|| ShapePlan::Circle((a + at) / 2.0, r))
            }
            (ToolKind::Circle3P, &[a, b], _) => shapes::circle_through(a, b, at, tol).map(circle),
            (ToolKind::Circle2T, _, &[l0, l1]) => {
                let diameter = self.input.locked(FieldKind::Diameter);
                tangent_circles::tangent_circle_two_lines(sk, l0, l1, at, diameter).map(circle)
            }
            (ToolKind::Circle3T, _, &[c0, c1]) => {
                tangent_circles::tangent_circle_three(sk, [c0, c1, third?], at).map(circle)
            }
            (ToolKind::PolygonInscribed, &[center], _) => {
                shapes::polygon_inscribed(center, at, self.current_sides(), tol)
                    .map(ShapePlan::Polygon)
            }
            (ToolKind::PolygonCircumscribed, &[center], _) => {
                shapes::polygon_circumscribed(center, at, self.current_sides(), tol)
                    .map(ShapePlan::Polygon)
            }
            (ToolKind::PolygonEdge, &[a, b], _) => {
                shapes::polygon_edge(a, b, at, self.current_sides(), tol).map(ShapePlan::Polygon)
            }
            (ToolKind::Arc3P, &[start, end], _) => {
                let radius = self.input.locked(FieldKind::Radius);
                if radius.is_some_and(|r| !radius_spans(start, end, r, tol)) {
                    return None;
                }
                shapes::arc_through(start, end, at, tol).map(ShapePlan::Arc)
            }
            (ToolKind::ArcTangent, &[_], _) => {
                let (.., from, leaving) = self.tangent_start(sk)?;
                shapes::tangent_arc(from, leaving, at, tol).map(ShapePlan::Arc)
            }
            (ToolKind::SlotCenterToCenter, &[a, b], _) => {
                shapes::slot_center_to_center(a, b, at, tol).map(ShapePlan::Slot)
            }
            (ToolKind::SlotOverall, &[a, b], _) => {
                shapes::slot_overall(a, b, at, tol).map(ShapePlan::Slot)
            }
            (ToolKind::SlotCenterPoint, &[c, end], _) => {
                shapes::slot_center_point(c, end, at, tol).map(ShapePlan::Slot)
            }
            (ToolKind::SlotThreePointArc | ToolKind::SlotCenterPointArc, &[a, b], _) => {
                let radius = self.input.locked(FieldKind::Radius);
                if radius.is_some_and(|r| !radius_spans(a, b, r, tol)) {
                    return None;
                }
                slot_axis(kind, a, b, at, tol).map(ShapePlan::Arc)
            }
            (ToolKind::SlotThreePointArc | ToolKind::SlotCenterPointArc, &[a, b, c], _) => {
                let axis = slot_axis(kind, a, b, c, tol)?;
                shapes::arc_slot(axis, at, tol).map(ShapePlan::ArcSlot)
            }
            _ => None,
        }
    }

    /// Vorschau der laufenden Form, aus demselben Plan wie das Ergebnis. Vor dem Plan zeigen
    /// `Rect3P`, `Circle3P`, `PolygonEdge` und alle Langlöcher die Linie zum Zeiger.
    pub fn kind_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>> {
        if let Some(kind) = self
            .planned_kind()
            .filter(|&k| k.is_fillet_or_chamfer() && k != ToolKind::Fillet)
        {
            let plan = self.chamfer_plan_at(sk, kind, at)?;
            return Some(vec![plan.touch.to_vec()]);
        }
        let shows_line = self.planned_kind().is_some_and(|k| {
            k.is_straight_slot()
                || k.is_arc_slot()
                || matches!(
                    k,
                    ToolKind::Rect3P | ToolKind::Circle3P | ToolKind::PolygonEdge
                )
        });
        if let (true, &[from]) = (shows_line, self.pending_pos.as_slice()) {
            return (from.distance(at) > sk.tolerance().linear).then(|| vec![vec![from, at]]);
        }
        Some(self.kind_plan(sk, at)?.outline(PREVIEW_STEPS_PER_TURN))
    }

    /// Lage mit den gesperrten Größen der laufenden Form; `None` ohne Sperre.
    pub(crate) fn kind_locked_position(&self, sk: &Sketch, cur: &DrawCursor) -> Option<DVec2> {
        use FieldKind as F;
        let locked = |kind| self.input.locked(kind);
        match (self.planned_kind()?, self.pending_pos.as_slice()) {
            (ToolKind::Rect3P, &[from]) => {
                let len = locked(F::Width);
                match locked(F::Angle) {
                    Some(angle) => Some(locked_ray(from, DVec2::X, cur, len, angle)),
                    None => Some(from + toward_pointer(cur, from) * len?),
                }
            }
            (ToolKind::Rect3P, &[a, b]) => {
                let depth = locked(F::Depth)?;
                let normal = (b - a).try_normalize()?.perp();
                let side = normal.dot(cur.raw - a).signum();
                Some(cur.at + normal * (side * depth - normal.dot(cur.at - a)))
            }
            (ToolKind::RectCenter, &[center]) => {
                let (w, h) = (locked(F::Width), locked(F::Height));
                if w.is_none() && h.is_none() {
                    return None;
                }
                let size = cur.at - center;
                Some(DVec2::new(
                    w.map_or(cur.at.x, |w| center.x + size.x.signum() * w / 2.0),
                    h.map_or(cur.at.y, |h| center.y + size.y.signum() * h / 2.0),
                ))
            }
            (ToolKind::Circle2P, &[from]) => {
                Some(from + toward_pointer(cur, from) * locked(F::Diameter)?)
            }
            (ToolKind::Circle2T, _) => {
                let d = locked(F::Diameter)?;
                let &[l0, l1] = self.pending_curves.as_slice() else {
                    return None;
                };
                let (m, _) =
                    tangent_circles::tangent_circle_two_lines(sk, l0, l1, cur.raw, Some(d))?;
                Some(m)
            }
            (ToolKind::PolygonInscribed | ToolKind::PolygonCircumscribed, &[center]) => {
                Some(center + toward_pointer(cur, center) * locked(F::Radius)?)
            }
            (ToolKind::PolygonEdge, &[from]) => {
                Some(from + toward_pointer(cur, from) * locked(F::Length)?)
            }
            (ToolKind::Arc3P, &[start, end]) => {
                let tol = sk.tolerance().linear;
                through_at_radius(start, end, locked(F::Radius)?, cur.raw, tol)
            }
            (ToolKind::ArcTangent, &[_]) => {
                let (radius, sweep) = (locked(F::Radius), locked(F::Sweep));
                if radius.is_none() && sweep.is_none() {
                    return None;
                }
                let (.., from, leaving) = self.tangent_start(sk)?;
                let tol = sk.tolerance().linear;
                // Ohne Lage (Zeiger auf der Tangente) bleibt nur der Anfang: kein Bogen, kein Plan.
                Some(
                    shapes::tangent_arc_locked(from, leaving, radius, sweep, cur.raw, tol)
                        .unwrap_or(from),
                )
            }
            (ToolKind::SlotCenterToCenter | ToolKind::SlotOverall, &[from]) => {
                Some(from + toward_pointer(cur, from) * locked(F::Length)?)
            }
            // Die Länge ist immer Mitte zu Mitte, der Zeiger gibt nur die Hälfte.
            (ToolKind::SlotCenterPoint, &[mid]) => {
                Some(mid + toward_pointer(cur, mid) * (locked(F::Length)? / 2.0))
            }
            (kind, &[a, b]) if kind.is_straight_slot() => {
                let half = locked(F::Width)? / 2.0;
                let normal = (b - a).try_normalize()?.perp();
                let side = normal.dot(cur.raw - a).signum();
                Some(cur.at + normal * (side * half - normal.dot(cur.at - a)))
            }
            (ToolKind::SlotThreePointArc, &[start, end]) => match locked(F::Radius) {
                Some(r) => through_at_radius(start, end, r, cur.raw, sk.tolerance().linear),
                None => through_at_sweep(start, end, locked(F::Sweep)?, cur.raw),
            },
            (ToolKind::SlotCenterPointArc, &[center]) => {
                Some(center + toward_pointer(cur, center) * locked(F::Radius)?)
            }
            (ToolKind::SlotCenterPointArc, &[center, start]) => {
                Some(center + DVec2::from_angle(locked(F::Sweep)?).rotate(start - center))
            }
            (kind, &[a, b, c]) if kind.is_arc_slot() => {
                let half = locked(F::Width)? / 2.0;
                let axis = slot_axis(kind, a, b, c, sk.tolerance().linear)?;
                let (m, r) = (axis.center, axis.radius());
                let along = (cur.raw - m).try_normalize()?;
                // Innen geht es nur, solange der Radius für die halbe Breite reicht; sonst käme die
                // Lage auf die andere Seite der Mitte und ergäbe still eine andere Breite.
                let inside = cur.raw.distance(m) < r && half < r;
                Some(m + along * if inside { r - half } else { r + half })
            }
            _ => None,
        }
    }

    /// Wert, den das Feld `kind` der laufenden Form vor dem ersten Zeichen zeigt.
    pub(crate) fn kind_field_value(&self, sk: &Sketch, kind: FieldKind) -> Option<f64> {
        use FieldKind as F;
        if kind == F::Sides {
            return Some(self.current_sides() as f64);
        }
        let at = self.draw_cursor.as_ref()?.at;
        match (self.planned_kind()?, self.pending_pos.as_slice(), kind) {
            (k, _, F::Radius | F::Distance | F::Distance2) if k.is_fillet_or_chamfer() => {
                self.pointer_to_corner(sk, at)
            }
            (ToolKind::ChamferDistanceAngle, _, F::Angle) => Some(CHAMFER_ANGLE_DEFAULT),
            (ToolKind::Rect3P, &[from], F::Width) => Some(from.distance(at)),
            (ToolKind::Rect3P, &[from], F::Angle) => {
                let along = at - from;
                Some(along.y.abs().atan2(along.x))
            }
            (ToolKind::Rect3P, &[a, b], F::Depth) => {
                Some((b - a).try_normalize()?.perp().dot(at - a).abs())
            }
            (ToolKind::RectCenter, &[center], F::Width) => Some(2.0 * (at.x - center.x).abs()),
            (ToolKind::RectCenter, &[center], F::Height) => Some(2.0 * (at.y - center.y).abs()),
            (ToolKind::Circle2P, &[from], F::Diameter) => Some(from.distance(at)),
            (ToolKind::PolygonInscribed | ToolKind::PolygonCircumscribed, &[c], F::Radius) => {
                Some(c.distance(at))
            }
            (ToolKind::PolygonEdge, &[from], F::Length) => Some(from.distance(at)),
            (ToolKind::Circle2T, _, F::Diameter) => match self.kind_plan(sk, at)? {
                ShapePlan::Circle(_, r) => Some(2.0 * r),
                _ => None,
            },
            (ToolKind::SlotCenterToCenter | ToolKind::SlotOverall, &[from], F::Length) => {
                Some(from.distance(at))
            }
            (ToolKind::SlotCenterPoint, &[mid], F::Length) => Some(2.0 * mid.distance(at)),
            (_, &[a, b], F::Width) => Some(2.0 * (b - a).try_normalize()?.perp().dot(at - a).abs()),
            (ToolKind::SlotCenterPointArc, &[center], F::Radius) => Some(center.distance(at)),
            (slot, &[a, b, c], F::Width) => {
                let axis = slot_axis(slot, a, b, c, sk.tolerance().linear)?;
                Some(2.0 * (axis.center.distance(at) - axis.radius()).abs())
            }
            (
                ToolKind::Arc3P
                | ToolKind::ArcTangent
                | ToolKind::SlotThreePointArc
                | ToolKind::SlotCenterPointArc,
                _,
                F::Radius | F::Sweep,
            ) => match self.kind_plan(sk, at)? {
                ShapePlan::Arc(arc) if kind == F::Radius => Some(arc.radius()),
                ShapePlan::Arc(arc) => Some(arc.sweep()),
                _ => None,
            },
            _ => None,
        }
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
