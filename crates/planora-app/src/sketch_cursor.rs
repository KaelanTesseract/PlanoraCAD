//! Aufgelöster Zeiger beim Zeichnen: Dieselbe Funktion bestimmt je Bild die Vorschau samt Zeichen
//! und beim Klick die Lage und die Anbindung, sodass genau das entsteht, was zu sehen war. Rein,
//! ohne Oberfläche; der Fang-Index wird nur neu gebaut, wenn sich die Skizze geändert hat.

use crate::msg::Msg;
use crate::sketch_state::{SketchSession, Tool, err};
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::snap::{AlignHit, Snap, SnapHit, SnapIndex};
use std::sync::Arc;

/// Reichweite der Ausrichtungslinien relativ zum Fangradius; bei vielen Punkten klebte der Zeiger
/// sonst ständig an irgendeiner Verlängerung.
const ALIGN_RADIUS_FACTOR: f64 = 0.6;

/// Einstellungen des Zeigers, die von außen kommen.
#[derive(Clone, Copy, Debug, Default)]
pub struct CursorOptions {
    /// Strg gehalten: kein Fang, keine Ausrichtung, kein Raster, keine automatische Abhängigkeit.
    /// Gilt bis zum nächsten Setzen.
    pub suppressed: bool,
    /// Weite des Raster-Fangs (`None`: kein Raster-Fang).
    pub grid_step: Option<f64>,
}

/// Automatische Abhängigkeit, die beim Abschluss entstehen würde.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoConstraint {
    Horizontal,
    Vertical,
    Parallel(CurveId),
    Perpendicular(CurveId),
    Tangent(CurveId),
}

/// Der aufgelöste Zeiger eines Bildes oder Klicks.
#[derive(Clone, Debug)]
pub struct DrawCursor {
    /// Lage des Zeigers ohne Fang.
    pub raw: DVec2,
    /// Lage, an der die Vorschau endet und der Klick setzt.
    pub at: DVec2,
    pub snap: Snap,
    /// Lage des Fangziels; die Anbindung gilt nur, wenn `at` sie trifft.
    pub snap_at: Option<DVec2>,
    pub guides: AlignHit,
    pub autos: Vec<AutoConstraint>,
}

impl DrawCursor {
    fn free(raw: DVec2) -> DrawCursor {
        DrawCursor {
            raw,
            at: raw,
            snap: Snap::None,
            snap_at: None,
            guides: AlignHit::default(),
            autos: Vec::new(),
        }
    }

    /// Fangart, wenn die Endlage die Fanglage trifft, sonst [`Snap::None`].
    fn binding(&self, tol: f64) -> Snap {
        match self.snap_at {
            Some(s) if s.distance(self.at) <= tol => self.snap,
            _ => Snap::None,
        }
    }

    /// Vorhandener Punkt, auf dem die Endlage liegt.
    pub(crate) fn point(&self, tol: f64) -> Option<PointId> {
        match self.binding(tol) {
            Snap::Point(p) | Snap::Center(p) => Some(p),
            _ => None,
        }
    }

    /// Lage und Anbindung zum späteren Abschluss merken (Ecken, die erst dann entstehen).
    pub(crate) fn remember(&self, tol: f64) -> SnapHit {
        SnapHit {
            snap: self.binding(tol),
            at: self.at,
        }
    }

    /// Zeiger aus einer gemerkten Lage; was es nicht mehr gibt, bindet nicht.
    pub(crate) fn remembered(hit: SnapHit, sk: &Sketch) -> DrawCursor {
        let alive = match hit.snap {
            Snap::Point(p) | Snap::Center(p) => sk.point_xy(p).is_ok(),
            Snap::Midpoint(c) | Snap::Curve(c) => sk.curve(c).is_some(),
            Snap::Intersection(a, b) => sk.curve(a).is_some() && sk.curve(b).is_some(),
            Snap::None | Snap::Origin | Snap::Grid => true,
        };
        DrawCursor {
            snap: if alive { hit.snap } else { Snap::None },
            snap_at: Some(hit.at),
            ..DrawCursor::free(hit.at)
        }
    }
}

impl SketchSession {
    /// Zeichnet das aktive Werkzeug neue Geometrie (Fang, Anbindung, Hilfslinien)? Auswahl und
    /// Kantenkopie nicht; Stutzen, Dehnen und Brechen haben ihre eigene Vorschau.
    pub fn is_drawing_tool(&self) -> bool {
        !matches!(
            self.tool,
            Tool::Select | Tool::ProjectEdge | Tool::Trim | Tool::Extend | Tool::Break
        )
    }

    /// Bestimmt aus der Rohlage die Lage, die Fangart und die Hilfslinien: Strg ⇒ nichts davon,
    /// sonst Geometrie-Fang, dann Ausrichtung und je noch freier Achse das Raster. Gleiche
    /// Eingaben ergeben ohne Änderung der Skizze dasselbe Ergebnis.
    pub fn resolve_cursor(&mut self, sk: &Sketch, raw: DVec2, radius: f64) -> DrawCursor {
        let mut cur = DrawCursor::free(raw);
        if !self.is_drawing_tool() || self.cursor_opts.suppressed || !raw.is_finite() {
            return cur;
        }
        let index = self.current_index(sk);
        if let Some(hit) = index.find(raw, radius) {
            cur.at = hit.at;
            cur.snap = hit.snap;
            cur.snap_at = Some(hit.at);
            return cur;
        }
        cur.guides = index.align(raw, radius * ALIGN_RADIUS_FACTOR, &self.align_skip(sk));
        if let Some((_, x)) = cur.guides.x {
            cur.at.x = x;
        }
        if let Some((_, y)) = cur.guides.y {
            cur.at.y = y;
        }
        let step = self.cursor_opts.grid_step;
        if let Some(step) = step.filter(|s| s.is_finite() && *s > 0.0)
            && (cur.guides.x.is_none() || cur.guides.y.is_none())
        {
            if cur.guides.x.is_none() {
                cur.at.x = (raw.x / step).round() * step;
            }
            if cur.guides.y.is_none() {
                cur.at.y = (raw.y / step).round() * step;
            }
            cur.snap = Snap::Grid;
            cur.snap_at = Some(cur.at);
        }
        cur
    }

    /// Bestimmt [`draw_cursor`](Self::draw_cursor) für dieses Bild (`None` ohne Zeiger über der
    /// Skizze und außerhalb der Zeichenwerkzeuge).
    pub fn update_cursor(&mut self, sk: &Sketch, raw: Option<DVec2>, radius: f64) {
        self.draw_cursor = raw
            .filter(|r| r.is_finite() && self.is_drawing_tool())
            .map(|r| self.resolve_cursor(sk, r, radius));
    }

    /// Fang-Index zum Stand von `sk`, wenn nötig neu gebaut.
    fn current_index(&mut self, sk: &Sketch) -> Arc<SnapIndex> {
        match &self.snap_index {
            Some(index) if index.revision() == sk.revision() => Arc::clone(index),
            _ => {
                let index = Arc::new(SnapIndex::build(sk));
                self.snap_index = Some(Arc::clone(&index));
                index
            }
        }
    }

    /// Punkte, an denen sich der Zeiger nicht ausrichtet: die laufenden und die erste Ecke des
    /// Rechtecks (die gibt es nur als Lage).
    fn align_skip(&self, sk: &Sketch) -> Vec<PointId> {
        let mut skip = self.pending.clone();
        if self.tool == Tool::Rectangle
            && let Some(&corner) = self.pending_pos.first()
        {
            let tol = sk.tolerance().linear;
            skip.extend(
                sk.points()
                    .map(|(p, _)| p)
                    .filter(|&p| sk.point_xy(p).is_ok_and(|q| q.distance(corner) <= tol)),
            );
        }
        skip
    }

    /// Bindet den Punkt `p` (`None`: einen neuen oder den gefangenen) an das Fangziel von `cur`.
    /// Punkt: ohne `p` derselbe Punkt, sonst `Coincident`; Kurve: `PointOnCurve`; Linienmitte:
    /// `Midpoint`; Bogenmitte: `PointOnCurve`, weil es `Midpoint` nur für Linien gibt; Schnitt:
    /// zweimal `PointOnCurve`; Ursprung: `Fixed(p, 0, 0)`. Trifft die Endlage die Fanglage nicht,
    /// bindet nichts.
    pub(crate) fn attach(
        &mut self,
        sk: &mut Sketch,
        p: Option<PointId>,
        cur: &DrawCursor,
    ) -> Result<PointId, Msg> {
        use ConstraintKind as K;
        let snap = cur.binding(sk.tolerance().linear);
        if let Snap::Point(q) | Snap::Center(q) = snap {
            let Some(own) = p else {
                return Ok(q);
            };
            if own != q {
                sk.add_constraint(K::Coincident(own, q)).map_err(err)?;
            }
            return Ok(own);
        }
        let own = match p {
            Some(own) => own,
            None => sk.add_point(cur.at.x, cur.at.y).map_err(err)?,
        };
        let kinds = match snap {
            Snap::Curve(c) => vec![K::PointOnCurve(own, c)],
            Snap::Midpoint(c) => match sk.curve(c).map(|d| &d.kind) {
                Some(CurveKind::Line { .. }) => vec![K::Midpoint(own, c)],
                _ => vec![K::PointOnCurve(own, c)],
            },
            Snap::Intersection(a, b) => vec![K::PointOnCurve(own, a), K::PointOnCurve(own, b)],
            Snap::Origin => vec![K::Fixed(own, 0.0, 0.0)],
            _ => Vec::new(),
        };
        for kind in kinds {
            sk.add_constraint(kind).map_err(err)?;
        }
        Ok(own)
    }

    /// Kreis oder Ellipse: Nur ein gefangener Punkt bindet, und nur, wenn er wirklich auf der
    /// neuen Kurve liegt (bei der kleinen Halbachse liegt der Klick im Allgemeinen daneben).
    pub(crate) fn bind_rim(
        &mut self,
        sk: &mut Sketch,
        curve: CurveId,
        cur: &DrawCursor,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let Some(p) = cur.point(tol) else {
            return Ok(());
        };
        let on_curve = sk.point_xy(p).is_ok_and(|q| {
            sk.nearest_on_curve(curve, q)
                .is_some_and(|n| n.distance(q) <= tol)
        });
        if on_curve {
            sk.add_constraint(ConstraintKind::PointOnCurve(p, curve))
                .map_err(err)?;
        }
        Ok(())
    }
}
