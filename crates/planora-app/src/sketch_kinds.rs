//! Die Arten der Zeichenwerkzeuge als Daten (Skizzen 3): je Art Werkzeug, Stufen samt Feldern,
//! Aktion, Name und Hinweis. Das Werkzeug bleibt [`Tool`]; die zuletzt gewählte Art je Form steht
//! in [`SketchSession::kinds`]. Die alten Vorgabearten laufen über ihre `click_*`-Funktionen, alle
//! anderen über eine einzige Stufenmaschine ([`SketchSession::click_kind`]): Sie sammelt Lagen und
//! legt die Form erst beim letzten Klick an, also ändert sie die Skizze bis dahin nicht.

use crate::keymap::Action;
use crate::msg::Msg;
use crate::sketch_cursor::DrawCursor;
use crate::sketch_input::{Created, FieldKind, locked_ray, toward_pointer};
use crate::sketch_state::{SketchSession, Snap, Tool, err};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::{self, ShapePlan};
use planora_sketch::tangent_circles;

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

    /// Die Meldung, wenn aus den Lagen keine Form wird. Die Arten späterer Tasks lehnen noch ab.
    fn refusal(self) -> Msg {
        use ToolKind::*;
        match self {
            Rect3P | RectCenter => crate::msg!("sketch-err-rect-size"),
            Circle2P => crate::msg!("sketch-radius-positive"),
            Circle3P => crate::msg!("sketch-circle-collinear"),
            Circle2T | Circle3T => crate::msg!("sketch-tangent-no-solution"),
            _ => crate::msg!("sketch-kind-pending"),
        }
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
            return self.pick_curve(sk, kind, cur);
        }
        let tol = sk.tolerance().linear;
        if self.pending_pos.iter().any(|p| p.distance(cur.at) <= tol) {
            return Err(kind.refusal());
        }
        self.pending_pos.push(cur.at);
        self.pending_snaps.push(cur.remember(tol));
        Ok(())
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

    /// Legt die Form in einem Zug an, bindet und löst, setzt `created` und `edit_label` und ruft
    /// `clear_pending`. Wird aus den Lagen keine Form, bleibt alles bis auf die Meldung, wie es war.
    fn finish_kind(
        &mut self,
        sk: &mut Sketch,
        kind: ToolKind,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
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
            .ok_or_else(|| kind.refusal())?;
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
            _ => return Err(kind.refusal()),
        };
        self.edit_label = Some(created.undo_label());
        self.created = Some(created);
        self.clear_pending();
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
    /// im Zeiger dieses Bilds ([`update_cursor`](Self::update_cursor)).
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
            _ => None,
        }
    }

    /// Vorschau der laufenden Form, aus demselben Plan wie das Ergebnis. Vor dem Plan zeigen
    /// `Rect3P` und `Circle3P` die Linie zum Zeiger.
    pub fn kind_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>> {
        if let (Some(ToolKind::Rect3P | ToolKind::Circle3P), &[from]) =
            (self.planned_kind(), self.pending_pos.as_slice())
        {
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
            _ => None,
        }
    }

    /// Wert, den das Feld `kind` der laufenden Form vor dem ersten Zeichen zeigt.
    pub(crate) fn kind_field_value(&self, sk: &Sketch, kind: FieldKind) -> Option<f64> {
        use FieldKind as F;
        if kind == F::Sides {
            return Some(self.input.sides.unwrap_or(self.polygon_sides) as f64);
        }
        let at = self.draw_cursor.as_ref()?.at;
        match (self.planned_kind()?, self.pending_pos.as_slice(), kind) {
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
            (ToolKind::Circle2T, _, F::Diameter) => match self.kind_plan(sk, at)? {
                ShapePlan::Circle(_, r) => Some(2.0 * r),
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
