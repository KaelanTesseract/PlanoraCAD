//! Zeichnen in 3D (Skizzen 10, Task 11): Linie, Spline (beide Arten) und Punkt setzen ihre Punkte
//! in der aktiven Ebene der Triade ([`triad_point`]), mit Achsfang, Z-Sperre (Pfeil hoch), Fang auf
//! vorhandene Punkte und den Ursprung über den Bildschirmabstand sowie den getippten Werten der
//! Felder am Zeiger ([`crate::sketch_input`]).
//!
//! Der 3D-Weg gilt nur, solange die Triade nicht die Skizzenebene ist oder die Z-Sperre gehalten
//! wird ([`SketchSession::draws_in_3d`]). Auf der Skizzenebene zeichnen die Werkzeuge wie in 2D,
//! mit Fang, Ausrichtung und Raster; die Triade springt dort ebenso auf jeden gesetzten Punkt.
//!
//! Ebene Formen (Task 12): Kreis „2 Punkte“, Bogen „3 Punkte“, Rechteck „2 Punkte“, Ellipse und
//! Kegelschnitt entstehen in der Ebene der Triade beim ersten Klick ([`Plane3`]). Gerechnet wird
//! in der Ebene als 2D-Basis mit den Formen der Ebene ([`planora_sketch::shapes`]), dann in
//! Skizzenkoordinaten gelegt. Fällt die Ebene mit der Skizzenebene zusammen (Normale ±Z durch
//! z = 0, gleich wie die Triade um Z gedreht ist), entsteht normale 2D-Geometrie; sonst bekommt
//! die Kurve die Normale der Ebene, und ihre Punkte sind 3D (Spec 1 und 4).
//!
//! Text (Task 13): Der Rahmen des Werkzeugs „Text“ entsteht ebenso in der Ebene der Triade
//! ([`crate::sketch_text::text_plane`]); OK legt ihn mit [`Sketch::add_text_box3`] an.
//!
//! Alle Arten (Task H, Handtest Etappe 2): Auch die übrigen Arten von Kreis, Bogen, Rechteck,
//! Polygon und Langloch zeichnen in der Ebene der Triade ([`Planar3::Kind`]). Ihre Lagen werden in
//! der 2D-Basis der Ebene gerechnet wie in 2D; Rechteck „3 Punkte“ und „Mittelpunkt“, Polygon und
//! Langloch entstehen dazu erst in einer Hilfsskizze mit den 2D-Formen und werden dann Punkt für
//! Punkt in die Ebene gelegt ([`transplant`]). Tangentenkreise und der tangentiale Bogen nehmen nur
//! Kurven, die in der Ebene liegen ([`curve_in_plane`]).

use crate::msg::Msg;
use crate::sketch_input::Created;
use crate::sketch_kinds::{PREVIEW_STEPS_PER_TURN, ToolKind, slot_axis};
use crate::sketch_state::{SketchSession, Tool, err};
use crate::sketch_triad::{AXIS_SNAP_PX, TriadAxis, triad_point};
use crate::sketch_view::{Mapper, PICK_RADIUS_PX};
use egui::Pos2;
use glam::Vec2;
use planora_math::{DVec2, DVec3};
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::helpers::{Included, RectIds};
use planora_sketch::model::{CurveId, CurveKind, MAX_SPLINE_POINTS, PointId, Sketch};
use planora_sketch::shapes::{
    self, ArcPlan, ArcSlotIds, PolygonIds, ShapePlan, SlotIds, SlotStyle,
};
use planora_sketch::{conic_apex_ok, conic_nurbs, tangent_circles};
use std::collections::{HashMap, HashSet};

/// Eine Körperecke, an der das Zeichnen in 3D fängt (A7, Task 16): Lage in Skizzenkoordinaten,
/// Körper (Index in [`crate::state::AppState::bodies`]) und die Kante, deren Anfang (`end ==
/// false`) oder Ende sie ist. Die App setzt die Liste beim Öffnen der Skizze.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyVertex {
    pub at: DVec3,
    pub body: usize,
    pub edge: planora_kernel::Id<planora_kernel::Edge>,
    pub end: bool,
}

/// Toleranz, unter der eine Achse der Triade als Achse der Skizze gilt (Triade unverdreht oder um
/// Vielfache von 90° gedreht).
const AXIS_TOL: f64 = 1e-9;

/// Der Zeiger beim Zeichnen in 3D, in Skizzenkoordinaten.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cursor3 {
    /// Punkt der Triade unter dem Zeiger (Ebene oder Z-Sperre, mit Achsfang), ohne Fang und Felder.
    pub raw: DVec3,
    /// Lage nach Fang und getippten Werten: Dort setzt der Klick.
    pub at: DVec3,
    /// Triadenachse, auf der der Punkt vom vorigen Punkt aus liegt (Achsfang; bei der Z-Sperre die
    /// Normale, wenn sie vom vorigen Punkt ausgeht).
    pub axis: Option<TriadAxis>,
    /// Vorhandener Punkt, auf den der Zeiger fängt; der Klick nimmt ihn statt eines neuen.
    pub snap: Option<PointId>,
    /// Fang auf den Skizzenursprung (ohne Punkt dort): Der neue Punkt wird dort fest.
    pub origin: bool,
    /// Fang auf die Mitte einer Linie oder eines Bogens (Skizzen 10, A7): Der neue Punkt bekommt
    /// dort `Midpoint` (Linie) bzw. `PointOnCurve` (Bogen), wie in 2D.
    pub mid: Option<CurveId>,
    /// Kurve unter dem Zeiger, solange die Art Kurven wählt (Kreis „2 Tangenten“, „3 Tangenten“,
    /// Task H).
    pub curve: Option<CurveId>,
    /// Fang an einer Körperecke (A7, Task 16; Index in [`SketchSession::body_vertices`]): Ein neuer
    /// Punkt dort bekommt einen eingezogenen, verknüpften Punkt mit „Deckungsgleich“.
    pub vertex: Option<usize>,
    /// Bildpunkt im Viewport, aus dem die Lage stammt (ohne Zeiger bleibt die Vorschau dort).
    pub pixel: Vec2,
}

impl Cursor3 {
    /// Zeiger ohne Fang an der Stelle `p`.
    pub fn free(p: DVec3, axis: Option<TriadAxis>) -> Cursor3 {
        Cursor3 {
            raw: p,
            at: p,
            axis,
            snap: None,
            origin: false,
            mid: None,
            curve: None,
            vertex: None,
            pixel: Vec2::ZERO,
        }
    }
}

impl SketchSession {
    /// Zeichnet das aktive Werkzeug gerade in 3D? In einer 3D-Skizze mit Linie, Punkt oder Spline
    /// (beide Arten), wenn die aktive Ebene der Triade nicht die Skizzenebene ist oder die
    /// Z-Sperre gehalten wird. Sonst zeichnet es wie in 2D.
    ///
    /// Ebene Formen ([`Planar3`]) zeichnen ebenso in 3D; eine in der Ebene begonnene Form endet
    /// in der Ebene, eine in 3D begonnene in 3D.
    pub fn draws_in_3d(&self, sk: &Sketch) -> bool {
        // Text (Task 13): der Rahmen in der Ebene der Triade.
        if self.tool == Tool::Text {
            return self.text_draws_in_3d(sk);
        }
        if sk.three_d && self.line_arc.is_none() && self.planar3().is_some() {
            // Eine in 3D begonnene Form hat ihre Ebene schon (auch nach gewählten Kurven).
            return self.plane3.is_some()
                || (self.pending.is_empty()
                    && self.pending_pos.is_empty()
                    && self.pending_curves.is_empty()
                    && (!self.triad.is_sketch_plane() || self.triad.lock_z));
        }
        sk.three_d
            && self.planned_kind().is_none()
            && self.line_arc.is_none()
            && matches!(self.tool, Tool::Line | Tool::Point | Tool::Spline)
            && (!self.triad.is_sketch_plane() || self.triad.lock_z)
    }

    /// Bestimmt [`cursor3`](Self::cursor3) für dieses Bild aus dem Zeiger `pointer` (ohne Zeiger
    /// aus dem letzten Bildpunkt). Beim Drücken von Pfeil hoch merkt es sich die Stelle, von der
    /// aus die Z-Sperre läuft; nahe am Bild der Triade ist das genau ihr Ursprung (der vorige
    /// Punkt). Außerhalb des 3D-Wegs bleibt alles wie in 2D.
    pub fn update_cursor3(&mut self, sk: &Sketch, map: &Mapper, pointer: Option<Pos2>) {
        self.draw3 = self.draws_in_3d(sk);
        if !self.triad.lock_z {
            self.lock_from = None;
        }
        if !self.draw3 {
            self.cursor3 = None;
            return;
        }
        let origin = self.triad.origin;
        if self.triad.lock_z && self.lock_from.is_none() {
            let base = self
                .cursor3
                .map(|c| c.at)
                .or_else(|| {
                    self.draw_cursor
                        .as_ref()
                        .map(|c| DVec3::new(c.at.x, c.at.y, 0.0))
                })
                .unwrap_or(origin);
            let close = map
                .to_screen3(base)
                .zip(map.to_screen3(origin))
                .is_some_and(|(a, b)| a.distance(b) <= AXIS_SNAP_PX);
            self.lock_from = Some(if close { origin } else { base });
        }
        // Die Vorschau und die Zeichen der Ebene gelten nicht.
        self.draw_cursor = None;
        let viewport = Vec2::new(map.rect.width(), map.rect.height());
        let pixel = pointer
            .map(|p| Vec2::new(p.x - map.rect.left(), p.y - map.rect.top()))
            .or(self.cursor3.map(|c| c.pixel));
        let Some(pixel) = pixel else {
            self.cursor3 = None;
            return;
        };
        let Some((raw, mut axis)) = triad_point(
            &self.triad,
            &map.camera,
            &map.frame,
            viewport,
            pixel,
            self.lock_from,
        ) else {
            // Streifender Blick: kein Punkt (der Klick nennt den Grund).
            self.cursor3 = None;
            return;
        };
        if self.triad.lock_z && self.lock_from == Some(origin) {
            axis = Some(self.triad.active.normal_axis());
        }
        let mut cur = Cursor3 {
            raw,
            at: raw,
            axis,
            snap: None,
            origin: false,
            mid: None,
            curve: None,
            vertex: None,
            pixel,
        };
        let at = Pos2::new(pixel.x + map.rect.left(), pixel.y + map.rect.top());
        if !self.triad.lock_z && !self.cursor_opts.suppressed {
            self.snap3(sk, map, at, &mut cur);
        }
        if !self.triad.lock_z && self.picks_curve3() {
            cur.curve = curve_under3(sk, map, at);
        }
        self.sync_input(sk);
        if let Some(locked) = self.locked_position3(sk, &cur) {
            let tol = sk.tolerance().linear;
            if locked.distance(cur.at) > tol {
                cur.snap = None;
                cur.origin = false;
                cur.mid = None;
                cur.vertex = None;
            }
            cur.at = locked;
            if self
                .input
                .locked(crate::sketch_input::FieldKind::Angle)
                .is_some()
            {
                cur.axis = None;
            }
        }
        self.track_arc_sweep3(cur.at);
        self.cursor3 = Some(cur);
    }

    /// Fang beim Zeichnen in 3D über den Bildschirmabstand (Spec 4, A7): zuerst der nächste
    /// vorhandene, sichtbare Punkt (ohne die Punkte der Buchstaben eines Textes, unsichtbare Griffe
    /// und verborgene Musterteile, wie [`crate::sketch_view::pick_3d`]) und der Skizzenursprung,
    /// dann die nächste Mitte einer sichtbaren Linie oder eines Bogens (2D wie 3D, ohne Buchstaben
    /// und Griffe),
    /// jeweils an ihrer Lage im Raum. Nach dem Ursprung fangen Körperecken (A7, Task 16): Ein
    /// neuer Punkt dort bekommt einen eingezogenen, verknüpften Punkt ([`bind_new3`]).
    fn snap3(&self, sk: &Sketch, map: &Mapper, at: Pos2, cur: &mut Cursor3) {
        let hidden = crate::sketch_view::hidden_in_3d(self, sk);
        let near = sk
            .points()
            .filter(|(p, _)| !hidden.points.contains(p))
            .filter_map(|(p, _)| {
                let q = sk.point_xyz(p).ok()?;
                let d = map.to_screen3(q)?.distance(at);
                (d <= PICK_RADIUS_PX).then_some((d, p, q))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p, q)) = near {
            cur.at = q;
            cur.snap = Some(p);
            cur.axis = None;
            return;
        }
        if map
            .to_screen3(DVec3::ZERO)
            .is_some_and(|s| s.distance(at) <= PICK_RADIUS_PX)
        {
            cur.at = DVec3::ZERO;
            cur.origin = true;
            cur.axis = None;
            return;
        }
        let corner = self
            .body_vertices
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                let d = map.to_screen3(v.at)?.distance(at);
                (d <= PICK_RADIUS_PX).then_some((d, i, v.at))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, i, q)) = corner {
            cur.at = q;
            cur.vertex = Some(i);
            cur.axis = None;
            return;
        }
        let glyph_curves = sk.glyph_curves();
        let mid = sk
            .curves()
            .filter(|(c, _)| {
                !glyph_curves.contains(c) && !sk.is_handle_curve(*c) && !hidden.curves.contains(c)
            })
            .filter_map(|(c, _)| {
                let m = curve_mid3(sk, c)?;
                let d = map.to_screen3(m)?.distance(at);
                (d <= PICK_RADIUS_PX).then_some((d, c, m))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, c, m)) = mid {
            cur.at = m;
            cur.mid = Some(c);
            cur.axis = None;
        }
    }

    /// Klick beim Zeichnen in 3D an der Stelle von [`cursor3`](Self::cursor3). Bei streifendem
    /// Blick setzt er nichts, und die Meldung bittet, die Ansicht zu drehen oder die Ebene zu
    /// wechseln. Atomar; ein Befehl ohne fertige Form hinterlässt bei Esc nichts.
    pub fn click3(&mut self, sk: &mut Sketch) -> Result<(), String> {
        // Nur mit Körperecken zum Fangen lohnt der Blick auf die neuen Punkte.
        let before: Option<HashSet<PointId>> =
            (!self.body_vertices.is_empty()).then(|| sk.points().map(|(p, _)| p).collect());
        let r = self.click3_inner(sk);
        if r.is_ok()
            && let Some(before) = before
        {
            self.note_vertex_includes(sk, &before);
        }
        r
    }

    /// Merkt sich die eingezogenen Eckpunkte, die der letzte Klick angelegt hat (A7): neue feste
    /// Punkte ohne Kurve, „Deckungsgleich“ mit einem anderen Punkt, an einer Körperecke. Die
    /// Verknüpfung im Dokument hängt [`crate::state::AppState::observe_sketch`] an, sobald der
    /// Befehl ein Schritt wird.
    pub(crate) fn note_vertex_includes(&mut self, sk: &Sketch, before: &HashSet<PointId>) {
        let tol = sk.tolerance().linear;
        let fresh: Vec<PointId> = sk
            .points()
            .map(|(p, _)| p)
            .filter(|p| !before.contains(p) && !self.included_vertices.iter().any(|(q, _)| q == p))
            .collect();
        for q in fresh {
            let pinned = sk
                .constraints()
                .any(|(_, c)| c.kind.fixed_point() == Some(q));
            let bound = sk.constraints().any(
                |(_, c)| matches!(c.kind, ConstraintKind::Coincident(a, b) if a == q || b == q),
            );
            let on_curve = sk.curves().any(|(_, c)| c.kind.points().contains(&q));
            if !pinned || !bound || on_curve {
                continue;
            }
            let Ok(at) = sk.point_xyz(q) else {
                continue;
            };
            if let Some(i) = self
                .body_vertices
                .iter()
                .position(|v| v.at.distance(at) <= tol)
            {
                self.included_vertices.push((q, i));
            }
        }
    }

    /// Der Klick in 3D ohne das Merken der Eckpunkte ([`click3`](Self::click3)).
    fn click3_inner(&mut self, sk: &mut Sketch) -> Result<(), String> {
        self.created = None;
        let Some(cur) = self.cursor3 else {
            let e = crate::msg!("sketch-3d-grazing");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        };
        if self.tool == Tool::Text {
            // Nur eine Ecke des Rahmens; die Skizze bleibt bis OK unverändert.
            return self.click_text3(sk, &cur).map_err(|e| {
                let text = e.text();
                self.message = Some(e);
                text
            });
        }
        if let Some(Planar3::Kind(kind)) = self.planar3() {
            return self.with_open_start(sk, |se, sk| {
                se.atomic(sk, |se, sk, prior| se.click_kind3(sk, prior, kind, &cur))
            });
        }
        if let Some(kind) = self.planar3() {
            return self.with_open_start(sk, |se, sk| {
                se.atomic(sk, |se, sk, prior| se.click_planar3(sk, prior, kind, &cur))
            });
        }
        self.with_open_start(sk, |se, sk| {
            se.atomic(sk, |se, sk, prior| se.click3_tool(sk, prior, &cur))
        })
        .map(|_| ())
    }

    /// Setzt mit dem aktiven Werkzeug (Linie, Spline, Punkt) einen Punkt an `p` (Skizzen-
    /// koordinaten), ohne Fang; `axis`: die Triadenachse, auf der `p` vom vorigen Punkt aus
    /// liegt. Liegt `p` in der Skizzenebene (|z| ≤ Toleranz), entsteht ein 2D-Punkt, gleich wie
    /// die Triade steht, sonst ein 3D-Punkt (Spec 1). Der Achsfang ergibt an einer Linie `Horizontal`,
    /// `Vertical` oder `AlongZ`, wenn die Triadenachse eine Achse der Skizze ist. Atomar wie ein
    /// Klick.
    pub fn place_point_3d(
        &mut self,
        sk: &mut Sketch,
        p: DVec3,
        axis: Option<TriadAxis>,
    ) -> Result<PointId, String> {
        self.created = None;
        let cur = Cursor3::free(p, axis);
        self.with_open_start(sk, |se, sk| {
            se.atomic(sk, |se, sk, prior| se.click3_tool(sk, prior, &cur))
        })
    }

    /// Der Klick in 3D selbst; liefert den gesetzten (oder gefangenen) Punkt.
    fn click3_tool(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        cur: &Cursor3,
    ) -> Result<PointId, Msg> {
        match self.tool {
            Tool::Point => {
                if let Some(q) = cur.snap {
                    return Ok(q);
                }
                let p = self.point3_at(sk, cur)?;
                self.solve_checked(sk, prior)?;
                self.follow_triad(sk, p);
                Ok(p)
            }
            Tool::Line => {
                let p = self.point3_at(sk, cur)?;
                let Some(&start) = self.pending.first() else {
                    self.pending = vec![p];
                    self.solve_checked(sk, prior)?;
                    self.follow_triad(sk, p);
                    return Ok(p);
                };
                if self.is_midpoint_line() {
                    return self.finish_midpoint_line3(sk, prior, start, p, cur);
                }
                let line = sk.add_line(start, p).map_err(err)?;
                self.solve_checked(sk, prior)?;
                if let Some(kind) = self.axis_constraint(line, cur.axis) {
                    self.try_auto(sk, kind);
                }
                self.line_chain = Some(line);
                self.line_after_arc = None;
                self.pending = vec![p];
                self.created = Some(Created::Line(line));
                self.follow_triad(sk, p);
                Ok(p)
            }
            Tool::Spline => {
                if self.pending.len() >= MAX_SPLINE_POINTS {
                    return Err(crate::msg!("sketch-spline-max", max = MAX_SPLINE_POINTS));
                }
                let p = self.point3_at(sk, cur)?;
                if self.pending.contains(&p) {
                    return Err(crate::msg!("sketch-spline-point-twice"));
                }
                self.pending.push(p);
                self.solve_checked(sk, prior)?;
                self.follow_triad(sk, p);
                Ok(p)
            }
            _ => Err(crate::msg!("sketch-invalid-position")),
        }
    }

    /// Zweiter Klick der Mittelpunktlinie in 3D (Skizzen 11a): `end` ist das gesetzte oder
    /// gefangene Ende, der Anfang liegt gespiegelt an der Mitte `mid` in derselben Ebene der
    /// Triade und fängt nicht. Wie in 2D folgt keine Kette.
    fn finish_midpoint_line3(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        mid: PointId,
        end: PointId,
        cur: &Cursor3,
    ) -> Result<PointId, Msg> {
        let (m, e) = (
            sk.point_xyz(mid).map_err(err)?,
            sk.point_xyz(end).map_err(err)?,
        );
        let mirrored = m * 2.0 - e;
        if mirrored.distance(e) <= sk.tolerance().linear {
            return Err(ToolKind::MidpointLine.refusal());
        }
        let start = self.point3_at(sk, &Cursor3::free(mirrored, None))?;
        let line = sk.add_midpoint_line(mid, start, end).map_err(err)?;
        self.solve_checked(sk, prior)?;
        if let Some(kind) = self.axis_constraint(line, cur.axis) {
            self.try_auto(sk, kind);
        }
        self.pending.clear();
        self.line_chain = None;
        self.line_after_arc = None;
        self.created = Some(Created::Line(line));
        self.follow_triad(sk, end);
        Ok(end)
    }

    /// Der Punkt für den Klick: der gefangene oder ein neuer (2D, wenn er in der Skizzenebene
    /// liegt, |z| ≤ Toleranz, gleich wie die Triade steht). Fang auf den Ursprung hält ihn dort fest: `Fixed` an einem 2D-,
    /// `Fixed3(0, 0, 0)` an einem 3D-Punkt.
    fn point3_at(&mut self, sk: &mut Sketch, cur: &Cursor3) -> Result<PointId, Msg> {
        if let Some(q) = cur.snap {
            return Ok(q);
        }
        // Punkte freier Kurven: nur die Lage zählt, nicht die Triade (Spec 1, Ruling Fix-Runde 1).
        let flat = cur.at.z.abs() <= sk.tolerance().linear;
        let p = if flat {
            sk.add_point(cur.at.x, cur.at.y)
        } else {
            sk.add_point3(cur.at)
        }
        .map_err(err)?;
        bind_new3(sk, p, cur.origin, cur.mid, cur.vertex.is_some())?;
        Ok(p)
    }

    /// Abhängigkeit aus dem Achsfang an der neuen Linie `line`: Liegt die gefangene Triadenachse
    /// auf einer Achse der Skizze, X ergibt `Horizontal`, Y `Vertical`, Z `AlongZ`; sonst keine.
    fn axis_constraint(&self, line: CurveId, axis: Option<TriadAxis>) -> Option<ConstraintKind> {
        let d = self.triad.axis(axis?);
        let along = |v: DVec3| d.dot(v).abs() >= 1.0 - AXIS_TOL;
        let l = LineOrPoints::Line(line);
        Some(if along(DVec3::X) {
            ConstraintKind::Horizontal(l)
        } else if along(DVec3::Y) {
            ConstraintKind::Vertical(l)
        } else if along(DVec3::Z) {
            ConstraintKind::AlongZ(l)
        } else {
            return None;
        })
    }
}

// ---- Ebene Formen in 3D (Task 12) ---------------------------------------------------------------

/// Eine ebene Form, die in 3D zeichnet (Spec 4): alle Arten von Kreis, Bogen, Rechteck, Polygon
/// und Langloch, dazu Ellipse und Kegelschnitt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Planar3 {
    /// Kreis „2 Punkte“: die Enden eines Durchmessers.
    Circle2P,
    /// Bogen „3 Punkte“: Start, Ende, Durchgang.
    Arc3P,
    /// Rechteck „2 Punkte“: zwei gegenüberliegende Ecken.
    Rect2P,
    /// Ellipse: Mitte, Ende der großen Halbachse, Lage der kleinen.
    Ellipse,
    /// Kegelschnitt: Start, Ende, Scheitel, dann bestätigt ein Klick das Rho.
    Conic,
    /// Die übrigen Arten von Kreis, Bogen, Rechteck, Polygon und Langloch (Task H): Klicks wie in
    /// 2D, gerechnet in der Ebene ([`SketchSession::click_kind3`]).
    Kind(ToolKind),
}

impl Planar3 {
    /// Klicks bis zur fertigen Form.
    fn clicks(self) -> usize {
        match self {
            Planar3::Circle2P | Planar3::Rect2P => 2,
            Planar3::Arc3P | Planar3::Ellipse => 3,
            Planar3::Conic => 4,
            Planar3::Kind(k) => kind_picks(k) + kind_positions(k),
        }
    }
}

/// Eine gesetzte Lage einer ebenen Form: in die Ebene gelegt, mit dem Fang des Klicks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit3 {
    /// Lage in Skizzenkoordinaten, in der Ebene der Form.
    pub at: DVec3,
    /// Vorhandener Punkt an dieser Lage (nur, wenn er in der Ebene liegt); eigene Punkte der Form
    /// nehmen ihn statt eines neuen.
    pub snap: Option<PointId>,
    /// Fang auf den Skizzenursprung: Ein neuer eigener Punkt wird dort fest.
    pub origin: bool,
    /// Fang auf die Mitte einer Linie oder eines Bogens: Ein neuer eigener Punkt wird dort
    /// gebunden.
    pub mid: Option<CurveId>,
    /// Fang an einer Körperecke (A7): Ein neuer eigener Punkt bekommt dort einen eingezogenen
    /// Punkt mit „Deckungsgleich“.
    pub vertex: bool,
}

/// Die Ebene einer ebenen Form als 2D-Basis in Skizzenkoordinaten: Ursprung und zwei Achsen,
/// gegen den Uhrzeigersinn um `u × v`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane3 {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    /// Die Skizzenebene selbst (Normale ±Z, durch z = 0)? Dann entsteht 2D-Geometrie, und die
    /// Achsen sind so gelegt, dass die Normale +Z ist.
    pub flat: bool,
}

impl Plane3 {
    /// Die aktive Ebene der Triade durch `at`.
    pub fn from_triad(triad: &crate::sketch_triad::Triad, at: DVec3, tol: f64) -> Plane3 {
        let (a, b) = triad.active.axes();
        let (mut u, mut v) = (triad.axis(a), triad.axis(b));
        let n = u.cross(v);
        let flat = n.x.abs() <= AXIS_TOL && n.y.abs() <= AXIS_TOL && at.z.abs() <= tol;
        let mut origin = at;
        if flat {
            if n.z < 0.0 {
                std::mem::swap(&mut u, &mut v);
            }
            // Genau in der Skizzenebene.
            u.z = 0.0;
            v.z = 0.0;
            u = u.normalize();
            v = v.normalize();
            origin.z = 0.0;
        }
        Plane3 { origin, u, v, flat }
    }

    /// Die Normale der Ebene (`u × v`).
    pub fn normal(&self) -> DVec3 {
        self.u.cross(self.v)
    }

    /// Lage in der Ebene (2D-Basis).
    pub fn to2(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }

    /// Lage in Skizzenkoordinaten.
    pub fn to3(&self, q: DVec2) -> DVec3 {
        let p = self.origin + self.u * q.x + self.v * q.y;
        if self.flat {
            DVec3::new(p.x, p.y, 0.0)
        } else {
            p
        }
    }

    /// `p` senkrecht in die Ebene gelegt.
    pub fn project(&self, p: DVec3) -> DVec3 {
        self.to3(self.to2(p))
    }

    /// Liegt die Richtung `d` auf einer Achse der Skizze? Dann die Abhängigkeit dazu an der
    /// Linie `line`: X ergibt `Horizontal`, Y `Vertical`, Z `AlongZ`.
    fn axis_kind(d: DVec3, line: CurveId) -> Option<ConstraintKind> {
        let along = |a: DVec3| d.dot(a).abs() >= 1.0 - AXIS_TOL;
        let l = LineOrPoints::Line(line);
        if along(DVec3::X) {
            Some(ConstraintKind::Horizontal(l))
        } else if along(DVec3::Y) {
            Some(ConstraintKind::Vertical(l))
        } else if along(DVec3::Z) {
            Some(ConstraintKind::AlongZ(l))
        } else {
            None
        }
    }
}

impl SketchSession {
    /// Die ebene Form des aktiven Werkzeugs in seiner Art, wenn sie in 3D zeichnet.
    pub fn planar3(&self) -> Option<Planar3> {
        let kind = self.kind_of(self.tool);
        match self.tool {
            Tool::Circle if kind == Some(ToolKind::Circle2P) => Some(Planar3::Circle2P),
            Tool::Arc if kind == Some(ToolKind::Arc3P) => Some(Planar3::Arc3P),
            Tool::Rectangle if kind == Some(ToolKind::Rect2P) => Some(Planar3::Rect2P),
            Tool::Spline if kind == Some(ToolKind::Conic) => Some(Planar3::Conic),
            Tool::Ellipse => Some(Planar3::Ellipse),
            Tool::Circle | Tool::Arc | Tool::Rectangle | Tool::Polygon | Tool::Slot => {
                kind.map(Planar3::Kind)
            }
            _ => None,
        }
    }

    /// Die Ebene der laufenden Form; vor dem ersten Klick die aktive Ebene der Triade durch `at`.
    fn plane3_at(&self, sk: &Sketch, at: DVec3) -> Plane3 {
        self.plane3
            .unwrap_or_else(|| Plane3::from_triad(&self.triad, at, sk.tolerance().linear))
    }

    /// Klick einer ebenen Form in 3D: Vor dem letzten Klick merkt er sich die Lage (die Skizze
    /// bleibt unverändert, Esc hinterlässt nichts), der letzte legt die Form an und löst.
    fn click_planar3(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        kind: Planar3,
        cur: &Cursor3,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let plane = self.plane3_at(sk, cur.at);
        let hit = plane_hit(sk, &plane, cur);
        let at = hit.at;
        let stage = self.pending3.len();
        // Der Klick des Kegelschnitts nach dem Scheitel bestätigt nur das Rho.
        let confirms = kind == Planar3::Conic && stage == 3;
        if !confirms && self.pending3.iter().any(|h| h.at.distance(at) <= tol) {
            return Err(planar3_refusal(kind));
        }
        if stage + 1 < kind.clicks() {
            self.check_planar3_stage(kind, &plane, at, tol)?;
            self.pending3.push(hit);
            self.plane3 = Some(plane);
            self.triad.moved_to(at);
            return Ok(());
        }
        let mut hits = self.pending3.clone();
        hits.push(hit);
        let created = self.create_planar3(sk, kind, &plane, &hits)?;
        self.solve_checked(sk, prior)?;
        if kind == Planar3::Conic {
            self.conic_rho = self.current_rho();
        } else {
            self.triad.moved_to(at);
        }
        self.edit_label = Some(created.undo_label());
        self.created = Some(created);
        self.clear_pending();
        Ok(())
    }

    /// Prüft eine Zwischenstufe: Die große Halbachse der Ellipse braucht eine Länge, der Scheitel
    /// des Kegelschnitts darf nicht auf der Sehne liegen.
    fn check_planar3_stage(
        &self,
        kind: Planar3,
        plane: &Plane3,
        at: DVec3,
        tol: f64,
    ) -> Result<(), Msg> {
        let ats: Vec<DVec2> = self.pending3.iter().map(|h| plane.to2(h.at)).collect();
        let q = plane.to2(at);
        match (kind, ats.as_slice()) {
            (Planar3::Ellipse, &[m]) if m.distance(q) <= tol => {
                Err(crate::msg!("sketch-major-axis-positive"))
            }
            (Planar3::Conic, &[start, end]) if !conic_apex_ok(start, q, end, tol) => {
                Err(crate::msg!("sketch-err-conic-apex"))
            }
            _ => Ok(()),
        }
    }

    /// Legt die ebene Form aus den Lagen `hits` in der Ebene `plane` an (ohne zu lösen).
    fn create_planar3(
        &mut self,
        sk: &mut Sketch,
        kind: Planar3,
        plane: &Plane3,
        hits: &[Hit3],
    ) -> Result<Created, Msg> {
        let tol = sk.tolerance().linear;
        let n = plane.normal();
        let q: Vec<DVec2> = hits.iter().map(|h| plane.to2(h.at)).collect();
        match (kind, hits, q.as_slice()) {
            (Planar3::Circle2P, _, &[a, b]) => {
                let r = a.distance(b) / 2.0;
                if r <= tol || !r.is_finite() {
                    return Err(planar3_refusal(kind));
                }
                let m = (a + b) / 2.0;
                let center = new_point(sk, plane, plane.to3(m))?;
                let circle = sk.add_circle3(center, r, n).map_err(err)?;
                // Ein gefangener Punkt auf dem Rand liegt auf dem Kreis (wie `bind_rim` in 2D).
                for h in hits {
                    bind_rim3(sk, plane, circle, h, |q| (q.distance(m) - r).abs())?;
                }
                Ok(Created::Circle(circle))
            }
            (Planar3::Arc3P, &[h0, h1, _], &[s, e, through]) => {
                let arc = shapes::arc_through(s, e, through, tol)
                    .ok_or_else(|| crate::msg!("sketch-circle-collinear"))?;
                let center = new_point(sk, plane, plane.to3(arc.center))?;
                let first = own_point(sk, plane, &h0)?;
                let second = own_point(sk, plane, &h1)?;
                let curve = if arc.start == s {
                    sk.add_arc3(center, first, second, n)
                } else {
                    sk.add_arc3(center, second, first, n)
                }
                .map_err(err)?;
                Ok(Created::Arc(curve))
            }
            (Planar3::Rect2P, &[h0, h1], &[a, c]) => {
                let size = c - a;
                if size.x.abs() <= tol || size.y.abs() <= tol {
                    return Err(planar3_refusal(kind));
                }
                let p0 = own_point(sk, plane, &h0)?;
                let p1 = new_point(sk, plane, plane.to3(DVec2::new(c.x, a.y)))?;
                let p2 = own_point(sk, plane, &h1)?;
                let p3 = new_point(sk, plane, plane.to3(DVec2::new(a.x, c.y)))?;
                let points = [p0, p1, p2, p3];
                let lines = [
                    sk.add_line(p0, p1).map_err(err)?,
                    sk.add_line(p1, p2).map_err(err)?,
                    sk.add_line(p2, p3).map_err(err)?,
                    sk.add_line(p3, p0).map_err(err)?,
                ];
                // Liegen die Achsen der Ebene auf Achsen der Skizze, halten H/V/Entlang Z die
                // Kanten wie beim 2D-Rechteck; sonst Senkrecht und Parallel wie „3 Punkte“.
                let axes = [plane.u, plane.v, plane.u, plane.v];
                let along: Option<Vec<ConstraintKind>> = axes
                    .iter()
                    .zip(lines)
                    .map(|(&d, l)| Plane3::axis_kind(d, l))
                    .collect();
                let constraints = along.unwrap_or_else(|| {
                    vec![
                        ConstraintKind::Perpendicular(lines[0], lines[1]),
                        ConstraintKind::Parallel(lines[0], lines[2]),
                        ConstraintKind::Parallel(lines[1], lines[3]),
                    ]
                });
                for k in constraints {
                    sk.add_constraint(k).map_err(err)?;
                }
                Ok(Created::Rectangle(RectIds { points, lines }))
            }
            (Planar3::Ellipse, &[h0, _, _], &[m, major_end, minor_at]) => {
                let major = major_end - m;
                let len = major.length();
                if len <= tol {
                    return Err(crate::msg!("sketch-major-axis-positive"));
                }
                let minor = (major / len).perp_dot(minor_at - m).abs().min(len);
                if minor <= tol {
                    return Err(crate::msg!("sketch-minor-axis-positive"));
                }
                let center = own_point(sk, plane, &h0)?;
                let major3 = plane.u * major.x + plane.v * major.y;
                let ellipse = sk.add_ellipse3(center, major3, minor, n).map_err(err)?;
                // Gefangene Punkte am Ende der großen Halbachse und bei der kleinen liegen auf der
                // Ellipse, wenn sie wirklich darauf liegen (bei der kleinen meist nicht).
                let dir = major / len;
                let gap = |q: DVec2| {
                    let d = q - m;
                    let (u, v) = (d.dot(dir) / len, dir.perp_dot(d) / minor);
                    ((u * u + v * v).sqrt() - 1.0).abs() * minor
                };
                for h in &hits[1..] {
                    bind_rim3(sk, plane, ellipse, h, gap)?;
                }
                Ok(Created::Ellipse(ellipse))
            }
            (Planar3::Conic, &[h0, h1, h2, _], _) => {
                let start = own_point(sk, plane, &h0)?;
                let end = own_point(sk, plane, &h1)?;
                let apex = own_point(sk, plane, &h2)?;
                let rho = self.current_rho();
                let curve = sk.add_conic(start, end, apex, rho).map_err(err)?;
                Ok(Created::Conic(curve))
            }
            _ => Err(planar3_refusal(kind)),
        }
    }

    /// Vorschau der laufenden ebenen Form mit dem Zeiger an `at`, in Skizzenkoordinaten: vor dem
    /// Plan die Linie zum Zeiger (Bogen, Ellipse, Kegelschnitt), dann die Form, wie der Klick sie
    /// anlegte. Leer ohne Lage.
    pub fn planar3_preview(&self, sk: &Sketch, at: DVec3) -> Vec<Vec<DVec3>> {
        let Some(kind) = self.planar3() else {
            return Vec::new();
        };
        if let Planar3::Kind(k) = kind {
            return self.kind_preview3(sk, k, at);
        }
        let tol = sk.tolerance().linear;
        let plane = self.plane3_at(sk, at);
        let mut q: Vec<DVec2> = self.pending3.iter().map(|h| plane.to2(h.at)).collect();
        // Nach dem Scheitel des Kegelschnitts zählt der Zeiger nicht mehr.
        if !(kind == Planar3::Conic && q.len() == 3) {
            q.push(plane.to2(at));
        }
        let flat: Vec<Vec<DVec2>> = match (kind, q.as_slice()) {
            (Planar3::Circle2P, &[a, b]) => {
                let r = a.distance(b) / 2.0;
                if r <= tol {
                    return Vec::new();
                }
                ShapePlan::Circle((a + b) / 2.0, r).outline(PREVIEW_STEPS_PER_TURN)
            }
            (Planar3::Rect2P, &[a, c]) => {
                let corners = [a, DVec2::new(c.x, a.y), c, DVec2::new(a.x, c.y), a];
                vec![corners.to_vec()]
            }
            (Planar3::Arc3P | Planar3::Ellipse | Planar3::Conic, &[a, b]) => vec![vec![a, b]],
            (Planar3::Arc3P, &[s, e, through]) => match shapes::arc_through(s, e, through, tol) {
                Some(arc) => ShapePlan::Arc(arc).outline(PREVIEW_STEPS_PER_TURN),
                None => vec![vec![s, e]],
            },
            (Planar3::Ellipse, &[m, major_end, minor_at]) => {
                let major = major_end - m;
                let len = major.length();
                if len <= tol {
                    return Vec::new();
                }
                let minor = (major / len).perp_dot(minor_at - m).abs().min(len);
                let b = major.perp() / len * minor;
                let steps = PREVIEW_STEPS_PER_TURN;
                vec![
                    (0..=steps)
                        .map(|k| {
                            let t = std::f64::consts::TAU * k as f64 / steps as f64;
                            m + major * t.cos() + b * t.sin()
                        })
                        .collect(),
                ]
            }
            (Planar3::Conic, &[start, end, apex]) => {
                if !conic_apex_ok(start, apex, end, tol) {
                    vec![vec![start, end]]
                } else {
                    match conic_nurbs(start, apex, end, self.current_rho()) {
                        Ok(curve) => {
                            let (lo, hi) = curve.domain();
                            let steps = PREVIEW_STEPS_PER_TURN / 2;
                            let line = (0..=steps)
                                .map(|k| {
                                    let p = curve.point(lo + (hi - lo) * k as f64 / steps as f64);
                                    DVec2::new(p.x, p.y)
                                })
                                .collect();
                            vec![vec![start, apex, end], line]
                        }
                        Err(_) => vec![vec![start, apex, end]],
                    }
                }
            }
            _ => Vec::new(),
        };
        flat.into_iter()
            .map(|line| line.into_iter().map(|p| plane.to3(p)).collect())
            .collect()
    }
}

/// Die Lage des Zeigers `cur` in die Ebene `plane` gelegt, mit seinem Fang, soweit er dort noch
/// zutrifft.
fn plane_hit(sk: &Sketch, plane: &Plane3, cur: &Cursor3) -> Hit3 {
    let tol = sk.tolerance().linear;
    let at = plane.project(cur.at);
    Hit3 {
        at,
        snap: cur
            .snap
            .filter(|&q| sk.point_xyz(q).is_ok_and(|p| p.distance(at) <= tol)),
        origin: cur.origin && at.length() <= tol,
        mid: cur.mid.filter(|_| cur.at.distance(at) <= tol),
        vertex: cur.vertex.is_some() && cur.at.distance(at) <= tol,
    }
}

/// Die Meldung, wenn aus den Lagen keine ebene Form wird.
fn planar3_refusal(kind: Planar3) -> Msg {
    match kind {
        Planar3::Circle2P => crate::msg!("sketch-radius-positive"),
        Planar3::Arc3P => crate::msg!("sketch-arc-end-on-start"),
        Planar3::Rect2P => crate::msg!("sketch-err-rect-size"),
        Planar3::Ellipse => crate::msg!("sketch-major-axis-positive"),
        Planar3::Conic => crate::msg!("sketch-err-conic-apex"),
        Planar3::Kind(k) => k.refusal(),
    }
}

/// Ein neuer Punkt der Form an `p`: in der Skizzenebene ein 2D-Punkt, sonst ein 3D-Punkt.
fn new_point(sk: &mut Sketch, plane: &Plane3, p: DVec3) -> Result<PointId, Msg> {
    if plane.flat {
        sk.add_point(p.x, p.y)
    } else {
        sk.add_point3(p)
    }
    .map_err(err)
}

/// Ein eigener Punkt der Form an einer gesetzten Lage: der gefangene vorhandene Punkt, sonst ein
/// neuer; der Fang auf den Ursprung hält den neuen dort fest (`Fixed` bzw. `Fixed3`), der Fang auf
/// eine Mitte bindet ihn dort ([`bind_new3`]).
fn own_point(sk: &mut Sketch, plane: &Plane3, hit: &Hit3) -> Result<PointId, Msg> {
    if let Some(q) = hit.snap {
        return Ok(q);
    }
    let p = new_point(sk, plane, hit.at)?;
    bind_new3(sk, p, hit.origin, hit.mid, hit.vertex)?;
    Ok(p)
}

/// Bindet den neuen Punkt `p` an seinen Fang in 3D: am Ursprung `Fixed` (2D-Punkt) bzw.
/// `Fixed3` (3D-Punkt), an der Mitte einer Linie `Midpoint`, eines Bogens `PointOnCurve` (es gibt
/// `Midpoint` nur für Linien, wie beim 2D-Fang). An einer Körperecke (`vertex`, A7) entsteht dort
/// ein eingezogener fester Punkt ([`Sketch::add_included`]) mit „Deckungsgleich“ zu `p`; seine
/// Verknüpfung merkt sich die Sitzung nach dem Klick
/// ([`SketchSession::note_vertex_includes`]).
pub(crate) fn bind_new3(
    sk: &mut Sketch,
    p: PointId,
    origin: bool,
    mid: Option<CurveId>,
    vertex: bool,
) -> Result<(), Msg> {
    if vertex {
        let at = sk.point_xyz(p).map_err(err)?;
        let ids = sk.add_included(&Included::Point(at)).map_err(err)?;
        if let Some(&q) = ids.points.first() {
            sk.add_constraint(ConstraintKind::Coincident(p, q))
                .map_err(err)?;
        }
    }
    if origin {
        let pin = if sk.is_3d_point(p) {
            ConstraintKind::Fixed3(p, 0.0, 0.0, 0.0)
        } else {
            ConstraintKind::Fixed(p, 0.0, 0.0)
        };
        sk.add_constraint(pin).map_err(err)?;
    }
    if let Some(c) = mid {
        let kind = match sk.curve(c).map(|d| &d.kind) {
            Some(CurveKind::Line { .. }) => ConstraintKind::Midpoint(p, c),
            Some(_) => ConstraintKind::PointOnCurve(p, c),
            None => return Ok(()),
        };
        sk.add_constraint(kind).map_err(err)?;
    }
    Ok(())
}

/// Bindet einen gefangenen Punkt der Lage `hit` mit `PointOnCurve` an die neue ebene Kurve
/// `curve`, wenn er wirklich auf ihr liegt: in der Ebene und mit `gap` (Abstand zur Kurve in der
/// 2D-Basis der Ebene) höchstens die Toleranz (wie `bind_rim` in 2D). Ohne gefangenen Punkt nichts.
fn bind_rim3(
    sk: &mut Sketch,
    plane: &Plane3,
    curve: CurveId,
    hit: &Hit3,
    gap: impl Fn(DVec2) -> f64,
) -> Result<(), Msg> {
    let tol = sk.tolerance().linear;
    let Some(q) = hit.snap else {
        return Ok(());
    };
    let Ok(at) = sk.point_xyz(q) else {
        return Ok(());
    };
    let off_plane = (at - plane.origin).dot(plane.normal()).abs();
    if off_plane <= tol && gap(plane.to2(at)) <= tol {
        sk.add_constraint(ConstraintKind::PointOnCurve(q, curve))
            .map_err(err)?;
    }
    Ok(())
}

/// Mitte einer Linie oder eines Bogens im Raum (Fangziel „Mittelpunkt“ in 3D); andere Arten haben
/// keine.
fn curve_mid3(sk: &Sketch, c: CurveId) -> Option<DVec3> {
    match sk.curve(c)?.kind {
        CurveKind::Line { a, b } => Some((sk.point_xyz(a).ok()? + sk.point_xyz(b).ok()?) * 0.5),
        CurveKind::Arc { .. } => {
            let (lo, hi) = sk.curve_domain3(c)?;
            sk.curve_point3(c, 0.5 * (lo + hi))
        }
        _ => None,
    }
    .filter(|m| m.is_finite())
}

// ---- Alle Arten in 3D (Task H) -------------------------------------------------------------------

/// Toleranz, unter der zwei Normalen als gleich (oder gegenläufig) gelten.
const NORMAL_TOL: f64 = 1e-6;

/// Wie viele Kurven die Art zuerst wählt (Kreis „2 Tangenten“, „3 Tangenten“).
fn kind_picks(kind: ToolKind) -> usize {
    match kind {
        ToolKind::Circle2T => 2,
        ToolKind::Circle3T => 3,
        _ => 0,
    }
}

/// Wie viele Lagen die Art nach den gewählten Kurven setzt (der letzte Klick schließt ab).
fn kind_positions(kind: ToolKind) -> usize {
    use ToolKind::*;
    match kind {
        Circle3T => 0,
        Circle2T => 1,
        CircleCenter | Circle2P | ArcTangent | Rect2P | RectCenter | PolygonInscribed
        | PolygonCircumscribed => 2,
        Circle3P | Arc3P | ArcCenter | Rect3P | PolygonEdge | SlotCenterToCenter | SlotOverall
        | SlotCenterPoint => 3,
        SlotThreePointArc | SlotCenterPointArc => 4,
        _ => 0,
    }
}

/// Ende des Bogens „Mittelpunkt“ aus Mitte `m`, Start `s` und der Richtung `e`: auf dem Radius.
fn arc_center_end(m: DVec2, s: DVec2, e: DVec2, tol: f64) -> Option<DVec2> {
    let r = m.distance(s);
    let end = m + (e - m).try_normalize()? * r;
    (r > tol && end.distance(s) > tol).then_some(end)
}

/// Bogen „Mittelpunkt“ aus Mitte `m`, Start `s` und der Richtung des Endes `e`, das Ende auf dem
/// Radius. Er folgt dem Zug: gegen den Uhrzeigersinn, mit `cw` im Uhrzeigersinn (dann läuft der
/// Plan vom Ende zum Start, wie `ArcPlan` es verlangt).
fn arc_center_plan(m: DVec2, s: DVec2, e: DVec2, cw: bool, tol: f64) -> Option<ArcPlan> {
    let end = arc_center_end(m, s, e, tol)?;
    let (start, end) = if cw { (end, s) } else { (s, end) };
    Some(ArcPlan {
        center: m,
        start,
        end,
    })
}

/// Der Plan der Lagen `q` (in der 2D-Basis der Ebene, die letzte ist der Zeiger) für die Arten, die
/// nur Lagen setzen; dieselben Formen wie in 2D. Ein Bogenlangloch zeigt mit drei Lagen seinen
/// Mittelbogen.
fn shape_plan3(kind: ToolKind, q: &[DVec2], sides: usize, cw: bool, tol: f64) -> Option<ShapePlan> {
    use ToolKind::*;
    match (kind, q) {
        (CircleCenter, &[m, p]) => {
            let r = m.distance(p);
            (r > tol && r.is_finite()).then_some(ShapePlan::Circle(m, r))
        }
        (Circle3P, &[a, b, c]) => {
            shapes::circle_through(a, b, c, tol).map(|(m, r)| ShapePlan::Circle(m, r))
        }
        (ArcCenter, &[m, s, e]) => arc_center_plan(m, s, e, cw, tol).map(ShapePlan::Arc),
        (Rect3P, &[a, b, c]) => shapes::rect_three_point(a, b, c, tol).map(ShapePlan::Quad),
        (RectCenter, &[m, c]) => shapes::rect_center(m, c, tol).map(ShapePlan::Quad),
        (PolygonInscribed, &[m, p]) => {
            shapes::polygon_inscribed(m, p, sides, tol).map(ShapePlan::Polygon)
        }
        (PolygonCircumscribed, &[m, p]) => {
            shapes::polygon_circumscribed(m, p, sides, tol).map(ShapePlan::Polygon)
        }
        (PolygonEdge, &[a, b, c]) => {
            shapes::polygon_edge(a, b, c, sides, tol).map(ShapePlan::Polygon)
        }
        (SlotCenterToCenter, &[a, b, c]) => {
            shapes::slot_center_to_center(a, b, c, tol).map(ShapePlan::Slot)
        }
        (SlotOverall, &[a, b, c]) => shapes::slot_overall(a, b, c, tol).map(ShapePlan::Slot),
        (SlotCenterPoint, &[a, b, c]) => {
            shapes::slot_center_point(a, b, c, tol).map(ShapePlan::Slot)
        }
        (k, &[a, b, c]) if k.is_arc_slot() => slot_axis(k, a, b, c, tol).map(ShapePlan::Arc),
        (k, &[a, b, c, d]) if k.is_arc_slot() => {
            shapes::arc_slot(slot_axis(k, a, b, c, tol)?, d, tol).map(ShapePlan::ArcSlot)
        }
        _ => None,
    }
}

/// Die Meldung, wenn der letzte Klick keine Form ergibt (wie `finish_refusal` in 2D).
fn finish_refusal3(kind: ToolKind) -> Msg {
    match kind {
        k if k.is_straight_slot() || k.is_arc_slot() => crate::msg!("sketch-slot-too-wide"),
        ToolKind::PolygonEdge => crate::msg!("sketch-polygon-edge-side"),
        ToolKind::Circle3P => crate::msg!("sketch-circle-collinear"),
        ToolKind::ArcTangent => crate::msg!("sketch-arc-tangent-straight"),
        k => k.refusal(),
    }
}

impl SketchSession {
    /// Läuft der Bogen „Mittelpunkt“ in 3D im Uhrzeigersinn (so gezogen)?
    fn arc_cw3(&self) -> bool {
        self.arc_sweep3.is_some_and(|(_, total)| total < 0.0)
    }

    /// Führt beim Bogen „Mittelpunkt“ in 3D nach dem Start den gezogenen Winkel des Zeigers `at`
    /// mit (Fusion: Der Bogen folgt dem Zug). Sonst vergessen.
    fn track_arc_sweep3(&mut self, at: DVec3) {
        let (Some(Planar3::Kind(ToolKind::ArcCenter)), Some(plane), [m, s]) =
            (self.planar3(), self.plane3, self.pending3.as_slice())
        else {
            self.arc_sweep3 = None;
            return;
        };
        let (m, s) = (plane.to2(m.at), plane.to2(s.at));
        let p = plane.to2(at) - m;
        if p.length() <= 0.0 || !p.is_finite() {
            return;
        }
        let angle = (s - m).angle_to(p);
        let (last, total) = self.arc_sweep3.unwrap_or((0.0, 0.0));
        let step = (angle - last + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        self.arc_sweep3 = Some((angle, total + step));
    }

    /// Wählt die laufende Art in 3D gerade eine Kurve (Kreis „2 Tangenten“, „3 Tangenten“)?
    pub fn picks_curve3(&self) -> bool {
        matches!(self.planar3(), Some(Planar3::Kind(k)) if self.pending_curves.len() < kind_picks(k))
    }

    /// Klick einer der übrigen Arten in 3D: erst die Kurven (Tangentenkreise), dann die Lagen in der
    /// Ebene. Vor dem letzten Klick bleibt die Skizze unverändert, der letzte legt die Form an.
    fn click_kind3(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        kind: ToolKind,
        cur: &Cursor3,
    ) -> Result<(), Msg> {
        if self.pending_curves.len() < kind_picks(kind) {
            return self.pick_curve3(sk, prior, kind, cur);
        }
        let tol = sk.tolerance().linear;
        let plane = self.plane3_at(sk, cur.at);
        let hit = plane_hit(sk, &plane, cur);
        let at = hit.at;
        if self.pending3.iter().any(|h| h.at.distance(at) <= tol) {
            return Err(kind.refusal());
        }
        if self.pending3.len() + 1 < kind_positions(kind) {
            self.check_kind3_stage(sk, kind, &plane, &hit)?;
            self.pending3.push(hit);
            self.plane3 = Some(plane);
            self.triad.moved_to(at);
            return Ok(());
        }
        let mut hits = self.pending3.clone();
        hits.push(hit);
        let curves = self.pending_curves.clone();
        let created = self.create_kind3(sk, prior, kind, &plane, &hits, &curves, plane.to2(at))?;
        self.finish_kind3(created, Some(at));
        Ok(())
    }

    /// Wählt die Kurve unter dem Zeiger: ohne Kurve nichts, eine Kurve außerhalb der Ebene der
    /// Triade ergibt einen Hinweis. Die dritte Kurve von „3 Tangenten“ schließt ab; die Lage des
    /// Zeigers wählt dann unter den Lösungen.
    fn pick_curve3(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        kind: ToolKind,
        cur: &Cursor3,
    ) -> Result<(), Msg> {
        let Some(c) = cur.curve else {
            return Ok(());
        };
        crate::sketch_kinds::check_pick(sk, kind, c)?;
        let tol = sk.tolerance().linear;
        let plane = self
            .plane3
            .unwrap_or_else(|| Plane3::from_triad(&self.triad, cur.raw, tol));
        if !curve_in_plane(sk, &plane, c) {
            return Err(crate::msg!("sketch-3d-tangent-plane"));
        }
        if self.pending_curves.contains(&c) {
            return Ok(());
        }
        let last = kind == ToolKind::Circle3T && self.pending_curves.len() + 1 == kind_picks(kind);
        if !last {
            self.pending_curves.push(c);
            self.plane3 = Some(plane);
            return Ok(());
        }
        let mut curves = self.pending_curves.clone();
        curves.push(c);
        let at = plane.to2(cur.raw);
        let created = self.create_kind3(sk, prior, kind, &plane, &[], &curves, at)?;
        self.finish_kind3(created, None);
        Ok(())
    }

    /// Prüft eine Zwischenstufe: Der tangentiale Bogen beginnt an einem Punkt, an dem eine Linie
    /// oder ein Bogen in der Ebene endet; der Mittelbogen eines Bogenlanglochs muss entstehen.
    fn check_kind3_stage(
        &mut self,
        sk: &Sketch,
        kind: ToolKind,
        plane: &Plane3,
        hit: &Hit3,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        if kind == ToolKind::ArcTangent && self.pending3.is_empty() {
            let prev = hit
                .snap
                .and_then(|p| tangent_start3(sk, p, plane))
                .ok_or_else(|| kind.refusal())?;
            if !curve_in_plane(sk, plane, prev) {
                return Err(crate::msg!("sketch-3d-tangent-plane"));
            }
            self.auto_tangent = Some(prev);
        }
        if kind.is_arc_slot()
            && let [a, b] = self.pending3.as_slice()
            && slot_axis(
                kind,
                plane.to2(a.at),
                plane.to2(b.at),
                plane.to2(hit.at),
                tol,
            )
            .is_none()
        {
            return Err(if kind == ToolKind::SlotThreePointArc {
                crate::msg!("sketch-circle-collinear")
            } else {
                kind.refusal()
            });
        }
        Ok(())
    }

    /// Nach der fertigen Form: Polygon merkt sich die Seitenzahl, die Triade springt auf den
    /// letzten gesetzten Punkt, ein Rückgängig-Schritt mit dem Namen der Form.
    fn finish_kind3(&mut self, created: Created, at: Option<DVec3>) {
        if matches!(created, Created::Polygon { .. }) {
            self.polygon_sides = self.current_sides();
        }
        if let Some(at) = at {
            self.triad.moved_to(at);
        }
        self.edit_label = Some(created.undo_label());
        self.created = Some(created);
        self.clear_pending();
    }

    /// Legt die Form der Art `kind` aus den Lagen `hits` und den gewählten Kurven `curves` in der
    /// Ebene `plane` an und löst; `at`: der Zeiger in der 2D-Basis der Ebene.
    #[allow(clippy::too_many_arguments)]
    fn create_kind3(
        &mut self,
        sk: &mut Sketch,
        prior: &Sketch,
        kind: ToolKind,
        plane: &Plane3,
        hits: &[Hit3],
        curves: &[CurveId],
        at: DVec2,
    ) -> Result<Created, Msg> {
        let tol = sk.tolerance().linear;
        let n = plane.normal();
        let q: Vec<DVec2> = hits.iter().map(|h| plane.to2(h.at)).collect();
        let created = match (kind, q.as_slice()) {
            (ToolKind::CircleCenter, &[m, rim]) => {
                let r = m.distance(rim);
                if r <= tol || !r.is_finite() {
                    return Err(kind.refusal());
                }
                let center = own_point(sk, plane, &hits[0])?;
                let circle = sk.add_circle3(center, r, n).map_err(err)?;
                bind_rim3(sk, plane, circle, &hits[1], |p| (p.distance(m) - r).abs())?;
                Created::Circle(circle)
            }
            (ToolKind::Circle3P, &[a, b, c]) => {
                let (m, r) = shapes::circle_through(a, b, c, tol)
                    .ok_or_else(|| crate::msg!("sketch-circle-collinear"))?;
                let center = new_point(sk, plane, plane.to3(m))?;
                let circle = sk.add_circle3(center, r, n).map_err(err)?;
                for h in hits {
                    bind_rim3(sk, plane, circle, h, |p| (p.distance(m) - r).abs())?;
                }
                Created::Circle(circle)
            }
            (ToolKind::Circle2T | ToolKind::Circle3T, _) => {
                let refused = || ToolKind::Circle3T.refusal();
                let (m, r) = self
                    .tangent_plan3(sk, kind, plane, curves, at)
                    .ok_or_else(refused)?;
                let m3 = plane.to3(m);
                let center = new_point(sk, plane, m3)?;
                let circle = sk.add_circle3(center, r, n).map_err(err)?;
                for &c in curves {
                    sk.add_constraint(ConstraintKind::Tangent(c, circle))
                        .map_err(err)?;
                }
                self.solve_checked(sk, prior).map_err(|_| refused())?;
                // Wandert die Mitte beim Lösen, wäre es eine andere Lösung als die Vorschau.
                if sk.point_xyz(center).map_err(err)?.distance(m3) > 10.0 * tol {
                    return Err(refused());
                }
                return Ok(Created::Circle(circle));
            }
            (ToolKind::ArcCenter, &[m, s, e]) => {
                let at_end = arc_center_end(m, s, e, tol).ok_or_else(|| kind.refusal())?;
                let center = own_point(sk, plane, &hits[0])?;
                let start = own_point(sk, plane, &hits[1])?;
                // Das Ende liegt auf dem Radius; es bindet nur, wenn der Klick genau dort lag.
                let end = if e.distance(at_end) <= tol {
                    own_point(sk, plane, &hits[2])?
                } else {
                    new_point(sk, plane, plane.to3(at_end))?
                };
                // Im Uhrzeigersinn gezogen läuft der Bogen vom Ende zum Start.
                let arc = if self.arc_cw3() {
                    sk.add_arc3(center, end, start, n)
                } else {
                    sk.add_arc3(center, start, end, n)
                };
                Created::Arc(arc.map_err(err)?)
            }
            (ToolKind::ArcTangent, &[_, _]) => {
                let straight = || crate::msg!("sketch-arc-tangent-straight");
                let prev = self.auto_tangent.ok_or_else(|| kind.refusal())?;
                let start = hits[0].snap.ok_or_else(|| kind.refusal())?;
                let plan = self.tangent_arc_plan3(sk, plane, at).ok_or_else(straight)?;
                let from = plane.to2(sk.point_xyz(start).map_err(err)?);
                let center = new_point(sk, plane, plane.to3(plan.center))?;
                let end = own_point(sk, plane, &hits[1])?;
                let arc = if plan.start == from {
                    sk.add_arc3(center, start, end, n)
                } else {
                    sk.add_arc3(center, end, start, n)
                }
                .map_err(err)?;
                sk.add_constraint(ConstraintKind::Tangent(arc, prev))
                    .map_err(err)?;
                Created::Arc(arc)
            }
            _ => {
                let plan = shape_plan3(kind, &q, self.current_sides(), self.arc_cw3(), tol)
                    .ok_or_else(|| finish_refusal3(kind))?;
                transplanted(sk, kind, plane, &plan, &q, hits)?
            }
        };
        self.solve_checked(sk, prior)?;
        Ok(created)
    }

    /// Mitte und Radius des Tangentenkreises an die Kurven `curves` (in der Ebene), der Zeiger
    /// `at` wählt unter den Lösungen: gerechnet wie in 2D an ihren Abbildern in der Ebene.
    fn tangent_plan3(
        &self,
        sk: &Sketch,
        kind: ToolKind,
        plane: &Plane3,
        curves: &[CurveId],
        at: DVec2,
    ) -> Option<(DVec2, f64)> {
        let (flat, ids) = flat_refs(sk, plane, curves)?;
        match (kind, ids.as_slice()) {
            (ToolKind::Circle2T, &[a, b]) => {
                tangent_circles::tangent_circle_two_lines(&flat, a, b, at, None)
            }
            (ToolKind::Circle3T, &[a, b, c]) => {
                tangent_circles::tangent_circle_three(&flat, [a, b, c], at)
            }
            _ => None,
        }
    }

    /// Der tangentiale Bogen vom gesetzten Anfang bis `at` (2D-Basis der Ebene).
    fn tangent_arc_plan3(&self, sk: &Sketch, plane: &Plane3, at: DVec2) -> Option<ArcPlan> {
        let prev = self.auto_tangent?;
        let start = self.pending3.first()?.snap?;
        let from = plane.to2(sk.point_xyz(start).ok()?);
        let d = leaving3(sk, prev, start)?;
        let leaving = DVec2::new(d.dot(plane.u), d.dot(plane.v)).try_normalize()?;
        shapes::tangent_arc(from, leaving, at, sk.tolerance().linear)
    }

    /// Vorschau der übrigen Arten in 3D mit dem Zeiger an `at`: vor dem Plan die Linie zum Zeiger
    /// (wie in 2D), dann die Form, wie der Klick sie anlegte.
    fn kind_preview3(&self, sk: &Sketch, kind: ToolKind, at: DVec3) -> Vec<Vec<DVec3>> {
        let tol = sk.tolerance().linear;
        let plane = self.plane3_at(sk, at);
        let mut q: Vec<DVec2> = self.pending3.iter().map(|h| plane.to2(h.at)).collect();
        let p = plane.to2(at);
        let steps = PREVIEW_STEPS_PER_TURN;
        let flat: Vec<Vec<DVec2>> = match kind {
            ToolKind::Circle2T | ToolKind::Circle3T => {
                let mut curves = self.pending_curves.clone();
                if kind == ToolKind::Circle3T
                    && let Some(c) = self.cursor3.and_then(|c| c.curve)
                    && !curves.contains(&c)
                {
                    curves.push(c);
                }
                if curves.len() < kind_picks(kind) {
                    return Vec::new();
                }
                self.tangent_plan3(sk, kind, &plane, &curves, p)
                    .map(|(m, r)| ShapePlan::Circle(m, r).outline(steps))
                    .unwrap_or_default()
            }
            ToolKind::ArcTangent if q.len() == 1 => self
                .tangent_arc_plan3(sk, &plane, p)
                .map(|a| ShapePlan::Arc(a).outline(steps))
                .unwrap_or_default(),
            ToolKind::ArcTangent => Vec::new(),
            _ => {
                let shows_line = kind.is_straight_slot()
                    || kind.is_arc_slot()
                    || matches!(
                        kind,
                        ToolKind::Rect3P
                            | ToolKind::Circle3P
                            | ToolKind::PolygonEdge
                            | ToolKind::ArcCenter
                    );
                match q.as_slice() {
                    &[from] if shows_line => {
                        if from.distance(p) > tol {
                            vec![vec![from, p]]
                        } else {
                            Vec::new()
                        }
                    }
                    _ => {
                        q.push(p);
                        shape_plan3(kind, &q, self.current_sides(), self.arc_cw3(), tol)
                            .map(|plan| plan.outline(steps))
                            .unwrap_or_default()
                    }
                }
            }
        };
        flat.into_iter()
            .map(|line| line.into_iter().map(|p| plane.to3(p)).collect())
            .collect()
    }
}

/// Liegt die Kurve `c` in der Ebene `plane`? Eine Linie mit beiden Enden, Kreis und Bogen mit der
/// Mitte und gleicher (oder gegenläufiger) Normale. Andere Arten nimmt keine Tangente in 3D.
pub(crate) fn curve_in_plane(sk: &Sketch, plane: &Plane3, c: CurveId) -> bool {
    let tol = sk.tolerance().linear;
    let n = plane.normal();
    let on = |p: PointId| {
        sk.point_xyz(p)
            .is_ok_and(|q| (q - plane.origin).dot(n).abs() <= tol)
    };
    match sk.curve(c).map(|d| &d.kind) {
        Some(&CurveKind::Line { a, b }) => on(a) && on(b),
        Some(&CurveKind::Circle { center, .. } | &CurveKind::Arc { center, .. }) => {
            on(center) && sk.curve_normal(c).dot(n).abs() >= 1.0 - NORMAL_TOL
        }
        _ => false,
    }
}

/// Ein Punkt der Hilfsskizze an der Lage von `p` in der 2D-Basis der Ebene.
fn flat_point(f: &mut Sketch, sk: &Sketch, plane: &Plane3, p: PointId) -> Option<PointId> {
    let q = plane.to2(sk.point_xyz(p).ok()?);
    f.add_point(q.x, q.y).ok()
}

/// Die Kurven `curves` (in der Ebene) als 2D-Kurven einer Hilfsskizze in der Basis der Ebene, in
/// derselben Reihenfolge: für die Rechnung der Tangentenkreise wie in 2D. Ein Bogen mit
/// gegenläufiger Normale läuft dort umgekehrt.
fn flat_refs(sk: &Sketch, plane: &Plane3, curves: &[CurveId]) -> Option<(Sketch, Vec<CurveId>)> {
    let mut f = Sketch::new(*sk.frame(), "", *sk.tolerance());
    let mut ids = Vec::new();
    for &c in curves {
        let id = match sk.curve(c)?.kind {
            CurveKind::Line { a, b } => {
                let pa = flat_point(&mut f, sk, plane, a)?;
                let pb = flat_point(&mut f, sk, plane, b)?;
                f.add_line(pa, pb).ok()?
            }
            CurveKind::Circle { center, .. } => {
                let m = flat_point(&mut f, sk, plane, center)?;
                f.add_circle(m, sk.radius(c)?).ok()?
            }
            CurveKind::Arc { center, start, end } => {
                let along = sk.curve_normal(c).dot(plane.normal()) > 0.0;
                let (s, e) = if along { (start, end) } else { (end, start) };
                let m = flat_point(&mut f, sk, plane, center)?;
                let ps = flat_point(&mut f, sk, plane, s)?;
                let pe = flat_point(&mut f, sk, plane, e)?;
                f.add_arc(m, ps, pe).ok()?
            }
            _ => return None,
        };
        ids.push(id);
    }
    Some((f, ids))
}

/// Richtung, in der die Linie oder der Bogen `c` im Punkt `p` endet (im Raum; wie
/// `leaving_direction` in 2D). `None`, wenn `c` dort nicht endet.
fn leaving3(sk: &Sketch, c: CurveId, p: PointId) -> Option<DVec3> {
    let at = sk.point_xyz(p).ok()?;
    let xyz = |q: PointId| sk.point_xyz(q).ok();
    let n = sk.curve_normal(c);
    let d = match sk.curve(c)?.kind {
        CurveKind::Line { a, b } if a == p => at - xyz(b)?,
        CurveKind::Line { a, b } if b == p => at - xyz(a)?,
        CurveKind::Arc { center, end, .. } if end == p => n.cross(at - xyz(center)?),
        CurveKind::Arc { center, start, .. } if start == p => -n.cross(at - xyz(center)?),
        _ => return None,
    };
    d.try_normalize()
}

/// Die Kurve, an der ein tangentialer Bogen in `p` anläuft: die jüngste Linie oder der jüngste
/// Bogen, die dort enden; Konstruktionslinien nur, wenn sonst nichts dort endet (wie in 2D).
/// Kurven in der Ebene `plane` gehen vor; endet dort keine, die jüngste andere (der Klick meldet
/// dann, dass sie nicht in der Ebene liegt).
fn tangent_start3(sk: &Sketch, p: PointId, plane: &Plane3) -> Option<CurveId> {
    sk.curves()
        .filter(|&(c, _)| leaving3(sk, c, p).is_some())
        .max_by_key(|&(c, d)| (curve_in_plane(sk, plane, c), !d.construction))
        .map(|(c, _)| c)
}

/// Die Kurve unter dem Zeiger `at` im Raum (Bildschirmabstand, ohne Buchstaben und Griffe).
fn curve_under3(sk: &Sketch, map: &Mapper, at: Pos2) -> Option<CurveId> {
    let glyphs = sk.glyph_curves();
    sk.curves()
        .filter(|(c, _)| !glyphs.contains(c) && !sk.is_handle_curve(*c))
        .filter_map(|(c, _)| {
            let path = crate::sketch_view::curve_screen_path(sk, c, map);
            let d = path
                .windows(2)
                .map(|w| crate::sketch_view::segment_distance(at, w[0], w[1]))
                .fold(f32::MAX, f32::min);
            (d <= PICK_RADIUS_PX).then_some((d, c))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

/// Punkte und Kurven der Hilfsskizze und ihre Gegenstücke in der Skizze.
struct Moved {
    points: HashMap<PointId, PointId>,
    curves: HashMap<CurveId, CurveId>,
}

impl Moved {
    fn p(&self, p: PointId) -> PointId {
        self.points[&p]
    }
    fn c(&self, c: CurveId) -> CurveId {
        self.curves[&c]
    }
}

/// Legt die Form aus dem Plan `plan` an: erst mit den 2D-Formen in einer Hilfsskizze in der Basis
/// der Ebene, dann in die Ebene gelegt ([`transplant`]). Gebunden werden wie in 2D die eigenen
/// Punkte an den Klicks; die Breite eines Langlochs bindet nie, der Seitenmittelpunkt des
/// umbeschriebenen Polygons und die Enden von „Gesamtlänge“ liegen auf ihrer Kurve.
fn transplanted(
    sk: &mut Sketch,
    kind: ToolKind,
    plane: &Plane3,
    plan: &ShapePlan,
    q: &[DVec2],
    hits: &[Hit3],
) -> Result<Created, Msg> {
    let mut tmp = Sketch::new(*sk.frame(), "", *sk.tolerance());
    let made = match (kind, plan) {
        (ToolKind::Rect3P, ShapePlan::Quad(c)) => {
            Created::Rectangle(tmp.add_rect_three_point(*c).map_err(err)?)
        }
        (ToolKind::RectCenter, ShapePlan::Quad(_)) => {
            Created::Rectangle(tmp.add_rect_center(q[0], q[1]).map_err(err)?.rect)
        }
        (_, ShapePlan::Polygon(p)) => Created::Polygon {
            ids: tmp.add_polygon(p).map_err(err)?,
            inscribed: kind == ToolKind::PolygonInscribed,
        },
        (_, ShapePlan::Slot(p)) => {
            let style = if kind == ToolKind::SlotCenterPoint {
                SlotStyle::CenterPoint
            } else {
                SlotStyle::CenterToCenter
            };
            Created::Slot(tmp.add_slot(p, style).map_err(err)?)
        }
        (_, ShapePlan::ArcSlot(p)) => Created::ArcSlot(tmp.add_arc_slot(p).map_err(err)?),
        _ => return Err(kind.refusal()),
    };
    let slot = kind.is_straight_slot() || kind.is_arc_slot();
    let binds = if slot {
        &hits[..hits.len().saturating_sub(1)]
    } else {
        hits
    };
    let rim = matches!(kind, ToolKind::PolygonCircumscribed | ToolKind::SlotOverall);
    let m = transplant(sk, plane, &tmp, binds, rim)?;
    Ok(match made {
        Created::Rectangle(r) => Created::Rectangle(RectIds {
            points: r.points.map(|p| m.p(p)),
            lines: r.lines.map(|c| m.c(c)),
        }),
        Created::Polygon { ids, inscribed } => Created::Polygon {
            ids: PolygonIds {
                center: m.p(ids.center),
                circle: m.c(ids.circle),
                points: ids.points.iter().map(|&p| m.p(p)).collect(),
                lines: ids.lines.iter().map(|&c| m.c(c)).collect(),
            },
            inscribed,
        },
        Created::Slot(ids) => Created::Slot(SlotIds {
            centers: ids.centers.map(|p| m.p(p)),
            sides: ids.sides.map(|c| m.c(c)),
            caps: ids.caps.map(|c| m.c(c)),
            axis: m.c(ids.axis),
            mid: ids.mid.map(|p| m.p(p)),
        }),
        Created::ArcSlot(ids) => Created::ArcSlot(ArcSlotIds {
            center: m.p(ids.center),
            ends: ids.ends.map(|p| m.p(p)),
            axis: m.c(ids.axis),
            outer: m.c(ids.outer),
            inner: m.c(ids.inner),
            caps: ids.caps.map(|c| m.c(c)),
        }),
        other => other,
    })
}

/// Legt Punkte, Kurven und Abhängigkeiten der Hilfsskizze `tmp` (2D-Basis der Ebene) in die Ebene
/// `plane` der Skizze. Ein eigener Punkt an einem Klick aus `binds` mit gefangenem Punkt wird
/// dieser Punkt; ein neuer Punkt dort bekommt Ursprung oder Mitte ([`bind_new3`]). Mit `rim`
/// liegt ein gefangener Punkt ohne eigenen Punkt auf der Kurve der Form, an der er liegt (eine
/// Seitenmitte mit `Midpoint`). „Waagerecht“ und „Vertikal“ der Ebene werden zur Achse der Skizze,
/// auf der die Achse der Ebene liegt; sonst hält `Senkrecht` die beiden Kanten.
fn transplant(
    sk: &mut Sketch,
    plane: &Plane3,
    tmp: &Sketch,
    binds: &[Hit3],
    rim: bool,
) -> Result<Moved, Msg> {
    let tol = sk.tolerance().linear;
    let n = plane.normal();
    let mut used = vec![false; binds.len()];
    let mut m = Moved {
        points: HashMap::new(),
        curves: HashMap::new(),
    };
    for (p, _) in tmp.points() {
        let q = tmp.point_xy(p).map_err(err)?;
        let hit = binds
            .iter()
            .position(|h| plane.to2(h.at).distance(q) <= tol);
        let id = match hit.and_then(|i| binds[i].snap.map(|s| (i, s))) {
            Some((i, s)) => {
                used[i] = true;
                s
            }
            None => {
                let id = new_point(sk, plane, plane.to3(q))?;
                if let Some(i) = hit {
                    used[i] = true;
                    bind_new3(sk, id, binds[i].origin, binds[i].mid, binds[i].vertex)?;
                }
                id
            }
        };
        m.points.insert(p, id);
    }
    for (c, d) in tmp.curves() {
        let id = match d.kind {
            CurveKind::Line { a, b } => sk.add_line(m.p(a), m.p(b)),
            CurveKind::Circle { center, .. } => {
                let r = tmp
                    .radius(c)
                    .ok_or_else(|| crate::msg!("sketch-radius-positive"))?;
                sk.add_circle3(m.p(center), r, n)
            }
            CurveKind::Arc { center, start, end } => {
                sk.add_arc3(m.p(center), m.p(start), m.p(end), n)
            }
            _ => return Err(crate::msg!("sketch-invalid-position")),
        }
        .map_err(err)?;
        if d.construction {
            sk.set_construction(id, true).map_err(err)?;
        }
        m.curves.insert(c, id);
    }
    let mut level = Vec::new();
    let mut upright = Vec::new();
    for (_, d) in tmp.constraints() {
        use ConstraintKind::*;
        let kind = match d.kind {
            Horizontal(LineOrPoints::Line(l)) => {
                level.push(m.c(l));
                continue;
            }
            Vertical(LineOrPoints::Line(l)) => {
                upright.push(m.c(l));
                continue;
            }
            Coincident(a, b) => Coincident(m.p(a), m.p(b)),
            PointOnCurve(a, c) => PointOnCurve(m.p(a), m.c(c)),
            Midpoint(a, c) => Midpoint(m.p(a), m.c(c)),
            Parallel(a, b) => Parallel(m.c(a), m.c(b)),
            Perpendicular(a, b) => Perpendicular(m.c(a), m.c(b)),
            Tangent(a, b) => Tangent(m.c(a), m.c(b)),
            Equal(a, b) => Equal(m.c(a), m.c(b)),
            _ => return Err(crate::msg!("sketch-invalid-position")),
        };
        sk.add_constraint(kind).map_err(err)?;
    }
    let along: Option<Vec<ConstraintKind>> = level
        .iter()
        .map(|&l| Plane3::axis_kind(plane.u, l))
        .chain(upright.iter().map(|&l| Plane3::axis_kind(plane.v, l)))
        .collect();
    match (along, level.first(), upright.first()) {
        (Some(kinds), ..) => {
            for k in kinds {
                sk.add_constraint(k).map_err(err)?;
            }
        }
        (None, Some(&a), Some(&b)) => {
            sk.add_constraint(ConstraintKind::Perpendicular(a, b))
                .map_err(err)?;
        }
        _ => {}
    }
    if rim {
        for (h, _) in binds.iter().zip(&used).filter(|(_, u)| !**u) {
            let Some(s) = h.snap else {
                continue;
            };
            let at = plane.to2(h.at);
            let on = tmp.curves().find_map(|(c, d)| {
                if d.construction {
                    return None;
                }
                if let CurveKind::Line { a, b } = d.kind {
                    let mid = (tmp.point_xy(a).ok()? + tmp.point_xy(b).ok()?) / 2.0;
                    if mid.distance(at) <= tol {
                        return Some(ConstraintKind::Midpoint(s, m.c(c)));
                    }
                }
                let near = tmp.nearest_on_curve(c, at)?;
                (near.distance(at) <= tol).then(|| ConstraintKind::PointOnCurve(s, m.c(c)))
            });
            if let Some(k) = on {
                sk.add_constraint(k).map_err(err)?;
            }
        }
    }
    Ok(m)
}
