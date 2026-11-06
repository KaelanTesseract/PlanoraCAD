//! Aufgelöster Zeiger beim Zeichnen: Dieselbe Funktion bestimmt je Bild die Vorschau samt Zeichen
//! und beim Klick die Lage und die Anbindung, sodass genau das entsteht, was zu sehen war. Rein,
//! ohne Oberfläche; der Fang-Index wird nur neu gebaut, wenn sich die Skizze geändert hat.

use crate::msg::Msg;
use crate::sketch_kinds::ToolKind;
use crate::sketch_state::{SketchSession, Tool, TransformStage, err};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::snap::{AlignHit, Snap, SnapHit, SnapIndex};
use planora_sketch::solver::SolveStatus;
use std::sync::Arc;

/// Reichweite der Ausrichtungslinien relativ zum Fangradius; bei vielen Punkten klebte der Zeiger
/// sonst ständig an irgendeiner Verlängerung.
const ALIGN_RADIUS_FACTOR: f64 = 0.6;

/// Winkel, unter dem eine neue Kurve beim Zeichnen als waagerecht, senkrecht, parallel,
/// rechtwinklig oder tangential gilt.
const AUTO_AXIS_ANGLE: f64 = 3.0 * std::f64::consts::PI / 180.0;

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

impl AutoConstraint {
    /// Die Abhängigkeit an der neuen Kurve `new`.
    pub(crate) fn kind(self, new: CurveId) -> ConstraintKind {
        use ConstraintKind as K;
        match self {
            AutoConstraint::Horizontal => K::Horizontal(LineOrPoints::Line(new)),
            AutoConstraint::Vertical => K::Vertical(LineOrPoints::Line(new)),
            AutoConstraint::Parallel(c) => K::Parallel(new, c),
            AutoConstraint::Perpendicular(c) => K::Perpendicular(new, c),
            AutoConstraint::Tangent(c) => K::Tangent(new, c),
        }
    }
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

/// Richtung, in der die Kurve `c` den Punkt `p` an ihrem Ende verlässt (Fortsetzung über das Ende
/// hinaus, normiert); `None`, wenn `p` kein Ende von `c` ist.
pub(crate) fn leaving_direction(sk: &Sketch, c: CurveId, p: PointId) -> Option<DVec2> {
    let at = sk.point_xy(p).ok()?;
    let dir = match sk.curve(c)?.kind {
        CurveKind::Line { a, b } if a == p => at - sk.point_xy(b).ok()?,
        CurveKind::Line { a, b } if b == p => at - sk.point_xy(a).ok()?,
        CurveKind::Arc { center, end, .. } if end == p => (at - sk.point_xy(center).ok()?).perp(),
        CurveKind::Arc { center, start, .. } if start == p => {
            -(at - sk.point_xy(center).ok()?).perp()
        }
        _ => return None,
    };
    dir.try_normalize()
}

/// Kurve, an der ein tangentialer Bogen in `p` anläuft: `prefer`, wenn sie dort endet, sonst die
/// jüngste Linie oder der jüngste Bogen, die in `p` enden. Konstruktionslinien nur, wenn sonst nichts
/// dort endet: Die Hilfslinien eines Öffnungswinkels sind immer jünger als ihr Bogen.
pub(crate) fn tangent_start_curve(
    sk: &Sketch,
    p: PointId,
    prefer: Option<CurveId>,
) -> Option<CurveId> {
    let ends_here = |c: CurveId| leaving_direction(sk, c, p).is_some();
    prefer.filter(|&c| ends_here(c)).or_else(|| {
        sk.curves()
            .filter(|&(c, _)| ends_here(c))
            // Bei Gleichstand nimmt `max_by_key` den letzten, also den jüngsten.
            .max_by_key(|(_, data)| !data.construction)
            .map(|(c, _)| c)
    })
}

/// Winkel zwischen zwei normierten Richtungen, 0…π.
fn angle_between(a: DVec2, b: DVec2) -> f64 {
    a.perp_dot(b).abs().atan2(a.dot(b))
}

/// Fund mit dem kleinsten Winkel bis `limit`, bei Gleichstand der zuerst genannte.
fn best_within<T>(found: impl Iterator<Item = (f64, T)>, limit: f64) -> Option<T> {
    found
        .filter(|(angle, _)| *angle <= limit)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, v)| v)
}

/// Linien, die über gemeinsame Endpunkte mit `line` zusammenhängen, samt `line`.
fn connected_lines(sk: &Sketch, line: CurveId) -> Vec<CurveId> {
    let mut found = vec![line];
    let mut next = 0;
    while let Some(&c) = found.get(next) {
        next += 1;
        let Some(CurveKind::Line { a, b }) = sk.curve(c).map(|d| &d.kind) else {
            continue;
        };
        let (a, b) = (*a, *b);
        for (other, data) in sk.curves() {
            if let CurveKind::Line { a: p, b: q } = data.kind
                && (p == a || p == b || q == a || q == b)
                && !found.contains(&other)
            {
                found.push(other);
            }
        }
    }
    found
}

/// Richtung einer neuen Linie von `start` zu `raw_end`: H/V vor tangential vor parallel und
/// rechtwinklig, jeweils innerhalb von 3°; liegt das Ende auf gefangener Geometrie
/// (`on_geometry`), gilt nur eine Richtung, die schon exakt stimmt, denn dort wird nichts mehr
/// verschoben. Tangential nur in Laufrichtung des Bogens, der in `start` endet. Bezug sind alle
/// Linien außer der laufenden Kette (`chain`: ihre letzte Linie). Die Richtung ist normiert und
/// zeigt zum Zeiger.
pub fn plan_direction(
    sk: &Sketch,
    tool: Tool,
    start: PointId,
    chain: Option<CurveId>,
    raw_end: DVec2,
    on_geometry: bool,
) -> Option<(AutoConstraint, DVec2)> {
    if tool != Tool::Line {
        return None;
    }
    let tol = sk.tolerance();
    let along = raw_end - sk.point_xy(start).ok()?;
    if along.length() <= tol.linear {
        return None;
    }
    let d = along.normalize();
    let limit = if on_geometry {
        tol.angular
    } else {
        AUTO_AXIS_ANGLE
    };
    // Eine Gerade hat zwei Richtungen; es zählt die, die zum Zeiger zeigt.
    let toward = |u: DVec2| if u.dot(d) < 0.0 { -u } else { u };
    if angle_between(d, toward(DVec2::X)) <= limit {
        return Some((AutoConstraint::Horizontal, toward(DVec2::X)));
    }
    if angle_between(d, toward(DVec2::Y)) <= limit {
        return Some((AutoConstraint::Vertical, toward(DVec2::Y)));
    }
    let tangent = best_within(
        sk.curves()
            .filter(|(_, data)| matches!(data.kind, CurveKind::Arc { .. }))
            .filter_map(|(c, _)| {
                let forward = leaving_direction(sk, c, start)?;
                Some((
                    angle_between(d, forward),
                    (AutoConstraint::Tangent(c), forward),
                ))
            }),
        limit,
    );
    if tangent.is_some() {
        return tangent;
    }
    let skip = chain.map_or_else(Vec::new, |c| connected_lines(sk, c));
    best_within(
        sk.curves()
            .filter(|(c, _)| !skip.contains(c))
            .filter_map(|(c, data)| {
                let CurveKind::Line { a, b } = data.kind else {
                    return None;
                };
                let u = (sk.point_xy(b).ok()? - sk.point_xy(a).ok()?).try_normalize()?;
                Some([
                    (
                        angle_between(d, toward(u)),
                        (AutoConstraint::Parallel(c), toward(u)),
                    ),
                    (
                        angle_between(d, toward(u.perp())),
                        (AutoConstraint::Perpendicular(c), toward(u.perp())),
                    ),
                ])
            })
            .flatten(),
        limit,
    )
}

/// Kurve, an der ein Bogen um `center` mit dem Anfang `start` tangential weiterläuft: Linie oder
/// Bogen, die in `start` enden, gegen den Uhrzeigersinn in dieselbe Richtung. Eine frei gesetzte
/// Mitte rückt beim Setzen des Anfangs zurecht (bis 3°), eine gebundene muss schon exakt stimmen.
fn arc_start_tangent(
    sk: &Sketch,
    center: PointId,
    start: PointId,
    center_free: bool,
) -> Option<CurveId> {
    let tol = sk.tolerance();
    let out = (sk.point_xy(start).ok()? - sk.point_xy(center).ok()?)
        .perp()
        .try_normalize()?;
    let limit = if center_free {
        AUTO_AXIS_ANGLE
    } else {
        tol.angular
    };
    best_within(
        sk.curves()
            .filter(|(_, data)| matches!(data.kind, CurveKind::Line { .. } | CurveKind::Arc { .. }))
            .filter_map(|(c, _)| Some((angle_between(out, leaving_direction(sk, c, start)?), c))),
        limit,
    )
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

    /// Bestimmt aus der Rohlage die Lage, die Fangart, die Hilfslinien und die automatischen
    /// Abhängigkeiten: Strg ⇒ nichts davon, sonst Geometrie-Fang, dann Richtung, Ausrichtung und je
    /// noch freier Achse das Raster; zuletzt, auch mit Strg, die gesperrten Größen. Gleiche
    /// Eingaben ergeben ohne Änderung der Skizze dasselbe Ergebnis.
    pub fn resolve_cursor(&mut self, sk: &Sketch, raw: DVec2, radius: f64) -> DrawCursor {
        let mut cur = self.resolve_unlocked(sk, raw, radius);
        if raw.is_finite() {
            self.apply_locks(sk, &mut cur);
        }
        cur
    }

    /// [`resolve_cursor`](Self::resolve_cursor) ohne die gesperrten Größen.
    fn resolve_unlocked(&mut self, sk: &Sketch, raw: DVec2, radius: f64) -> DrawCursor {
        let mut cur = DrawCursor::free(raw);
        if !self.is_drawing_tool() || !raw.is_finite() {
            return cur;
        }
        // Verschieben/Kopieren und Maßstab: Nur der Bezugspunkt fängt. Beim Wählen zeigt der Zeiger
        // nur, was der Klick träfe, ganz ohne Fang; die Griffe rechnen mit der Rohlage.
        if matches!(self.tool, Tool::MoveCopy | Tool::Scale)
            && self.transform.stage != TransformStage::SetBase
        {
            if self.transform.stage == TransformStage::Picking {
                cur.snap = self.snap(sk, raw, radius);
            }
            return cur;
        }
        // Bemaßen und Muster: Der Zeiger zeigt nur, was der Klick wählte; die Art wählt die
        // Rohlage, das Muster rechnet ohne Fang.
        if matches!(self.tool, Tool::Dimension | Tool::Pattern) {
            cur.snap = self.snap(sk, raw, radius);
            return cur;
        }
        // Wer Kurven wählt, sieht nur Kurven: kein Punkt davor, keine Hilfslinie, kein Raster. Eine
        // Kurve wählen ist kein Fang, also geht das auch mit Strg.
        if self.picks_curves() {
            if let Some(c) = sk.pick_curve(raw, radius) {
                cur.at = sk.nearest_on_curve(c, raw).unwrap_or(raw);
                cur.snap = Snap::Curve(c);
                cur.snap_at = Some(cur.at);
            }
            return cur;
        }
        // Beim 2-Tangenten-Kreis wählt der letzte Klick nur den Sektor: Fang auf einer der Linien
        // ergäbe gar keinen. Bei Rundung und Fase gibt der Zeiger nur den Abstand zur Ecke an, beim
        // Versatz Abstand und Seite zur Kette.
        if self.cursor_opts.suppressed
            || self.planned_kind() == Some(ToolKind::Circle2T)
            || matches!(self.tool, Tool::Fillet | Tool::Offset)
        {
            return cur;
        }
        let index = self.current_index(sk);
        if let Some(hit) = index.find(raw, radius) {
            cur.at = hit.at;
            cur.snap = hit.snap;
            cur.snap_at = Some(hit.at);
            cur.autos
                .extend(self.line_direction(sk, hit.at, true).map(|d| d.0));
        } else {
            self.resolve_free(sk, &index, radius, &mut cur);
        }
        if self.tool == Tool::Arc && self.auto_allowed() {
            cur.autos.extend(self.arc_tangent(sk, &cur));
        }
        cur
    }

    /// Zeiger ohne Geometrie-Fang: erst die Richtung (das Ende wird exakt auf sie projiziert),
    /// dann Ausrichtung und Raster auf den Achsen, die sie frei lässt; eine schräge Richtung lässt
    /// keine frei.
    fn resolve_free(&mut self, sk: &Sketch, index: &SnapIndex, radius: f64, cur: &mut DrawCursor) {
        let raw = cur.raw;
        let (mut free_x, mut free_y) = (true, true);
        if let Some((auto, dir)) = self.line_direction(sk, raw, false)
            && let Some(from) = self.pending.first().and_then(|&p| sk.point_xy(p).ok())
        {
            cur.at = from + dir * (raw - from).dot(dir);
            cur.autos.push(auto);
            free_x = auto == AutoConstraint::Horizontal;
            free_y = auto == AutoConstraint::Vertical;
        }
        cur.guides = index.align(raw, radius * ALIGN_RADIUS_FACTOR, &self.align_skip(sk));
        if !free_x {
            cur.guides.x = None;
        }
        if !free_y {
            cur.guides.y = None;
        }
        if let Some((_, x)) = cur.guides.x {
            cur.at.x = x;
        }
        if let Some((_, y)) = cur.guides.y {
            cur.at.y = y;
        }
        let (grid_x, grid_y) = (
            free_x && cur.guides.x.is_none(),
            free_y && cur.guides.y.is_none(),
        );
        let step = self.cursor_opts.grid_step;
        if let Some(step) = step.filter(|s| s.is_finite() && *s > 0.0)
            && (grid_x || grid_y)
        {
            if grid_x {
                cur.at.x = (raw.x / step).round() * step;
            }
            if grid_y {
                cur.at.y = (raw.y / step).round() * step;
            }
            cur.snap = Snap::Grid;
            cur.snap_at = Some(cur.at);
        }
    }

    /// Darf eine Richtung entstehen? Schalter an und die Skizze nicht ungelöst; was
    /// [`try_auto`](Self::try_auto) ohnehin verwerfen würde, bekommt gar nicht erst ein Zeichen.
    fn auto_allowed(&self) -> bool {
        self.auto_constrain
            && self
                .last_report
                .as_ref()
                .is_none_or(|r| r.status == SolveStatus::Solved)
    }

    /// Richtung des laufenden Linienendes. Während des Zugs auf dem letzten Punkt keine; nach einem
    /// Bogen aus dem Linienzug läuft die Linie unabhängig von der 3°-Grenze tangential, solange der
    /// Zeiger vor dem Bogenende liegt und das Ende nicht auf gefangener Geometrie.
    fn line_direction(
        &self,
        sk: &Sketch,
        end: DVec2,
        on_geometry: bool,
    ) -> Option<(AutoConstraint, DVec2)> {
        if !self.auto_allowed() || self.line_arc.is_some() {
            return None;
        }
        let &start = self.pending.first()?;
        if !on_geometry
            && let Some(arc) = self.line_after_arc
            && let Some(forward) = leaving_direction(sk, arc, start)
            && forward.dot(end - sk.point_xy(start).ok()?) > sk.tolerance().linear
        {
            return Some((AutoConstraint::Tangent(arc), forward));
        }
        plan_direction(sk, self.tool, start, self.line_chain, end, on_geometry)
    }

    /// Tangente des Bogens: beim Setzen des Anfangs aus dem Punkt unter dem Zeiger, danach die
    /// beim Setzen entschiedene.
    fn arc_tangent(&self, sk: &Sketch, cur: &DrawCursor) -> Option<AutoConstraint> {
        let tangent = match *self.pending.as_slice() {
            [center] => {
                let start = cur.point(sk.tolerance().linear)?;
                arc_start_tangent(sk, center, start, self.arc_center_free())?
            }
            [_, _, ..] => self.auto_tangent.filter(|&c| sk.curve(c).is_some())?,
            _ => return None,
        };
        Some(AutoConstraint::Tangent(tangent))
    }

    /// Wurde die Mitte des laufenden Bogens frei gesetzt (ohne Bindung an Geometrie)?
    fn arc_center_free(&self) -> bool {
        self.pending_snaps
            .first()
            .is_some_and(|hit| matches!(hit.snap, Snap::None | Snap::Grid))
    }

    /// Bogenanfang gesetzt: Zeigt `cur` eine Tangente, gilt sie bis zum Abschluss. Eine freie Mitte
    /// rückt auf die Senkrechte durch den Anfang, damit das Lösen beim Abschluss nichts bewegt und
    /// die Vorschau schon stimmt.
    pub(crate) fn plan_arc_tangent(
        &mut self,
        sk: &mut Sketch,
        center: PointId,
        start: PointId,
        cur: &DrawCursor,
    ) -> Result<(), Msg> {
        let Some(c) = cur.autos.iter().find_map(|a| match a {
            AutoConstraint::Tangent(c) => Some(*c),
            _ => None,
        }) else {
            return Ok(());
        };
        self.auto_tangent = Some(c);
        if self.arc_center_free()
            && let Some(leaving) = leaving_direction(sk, c, start)
        {
            let s = sk.point_xy(start).map_err(err)?;
            let m = sk.point_xy(center).map_err(err)?;
            let normal = leaving.perp();
            let exact = s + normal * (m - s).dot(normal);
            sk.set_point_xy(center, exact.x, exact.y).map_err(err)?;
        }
        Ok(())
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

    /// Punkte, an denen sich der Zeiger nicht ausrichtet: die laufenden und die, die auf einer
    /// gemerkten Lage des Rechtecks oder einer geplanten Art liegen (die Form gibt es nur als
    /// Lagen).
    fn align_skip(&self, sk: &Sketch) -> Vec<PointId> {
        let mut skip = self.pending.clone();
        if self.tool == Tool::Rectangle || self.planned_kind().is_some() {
            let tol = sk.tolerance().linear;
            for &spot in &self.pending_pos {
                skip.extend(
                    sk.points()
                        .map(|(p, _)| p)
                        .filter(|&p| sk.point_xy(p).is_ok_and(|q| q.distance(spot) <= tol)),
                );
            }
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
