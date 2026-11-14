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

use crate::msg::Msg;
use crate::sketch_input::Created;
use crate::sketch_kinds::{PREVIEW_STEPS_PER_TURN, ToolKind};
use crate::sketch_state::{SketchSession, Tool, err};
use crate::sketch_triad::{AXIS_SNAP_PX, TriadAxis, triad_point};
use crate::sketch_view::{Mapper, PICK_RADIUS_PX};
use egui::Pos2;
use glam::Vec2;
use planora_math::{DVec2, DVec3};
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::helpers::RectIds;
use planora_sketch::model::{CurveId, MAX_SPLINE_POINTS, PointId, Sketch};
use planora_sketch::shapes::{self, ShapePlan};
use planora_sketch::{conic_apex_ok, conic_nurbs};
use std::collections::HashSet;

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
            return !self.pending3.is_empty()
                || (self.pending.is_empty()
                    && self.pending_pos.is_empty()
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
            pixel,
        };
        if !self.triad.lock_z && !self.cursor_opts.suppressed {
            let at = Pos2::new(pixel.x + map.rect.left(), pixel.y + map.rect.top());
            self.snap3(sk, map, at, &mut cur);
        }
        self.sync_input(sk);
        if let Some(locked) = self.locked_position3(sk, &cur) {
            let tol = sk.tolerance().linear;
            if locked.distance(cur.at) > tol {
                cur.snap = None;
                cur.origin = false;
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
        self.cursor3 = Some(cur);
    }

    /// Fang beim Zeichnen in 3D über den Bildschirmabstand: zuerst der nächste vorhandene Punkt
    /// (ohne die Punkte der Buchstaben eines Textes), dann der Skizzenursprung.
    fn snap3(&self, sk: &Sketch, map: &Mapper, at: Pos2, cur: &mut Cursor3) {
        let glyphs: HashSet<PointId> = sk
            .curves()
            .filter(|(c, _)| sk.text_of_curve(*c).is_some())
            .flat_map(|(_, d)| d.kind.points())
            .collect();
        let near = sk
            .points()
            .filter(|(p, _)| !glyphs.contains(p))
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
        } else if map
            .to_screen3(DVec3::ZERO)
            .is_some_and(|s| s.distance(at) <= PICK_RADIUS_PX)
        {
            cur.at = DVec3::ZERO;
            cur.origin = true;
            cur.axis = None;
        }
    }

    /// Klick beim Zeichnen in 3D an der Stelle von [`cursor3`](Self::cursor3). Bei streifendem
    /// Blick setzt er nichts, und die Meldung bittet, die Ansicht zu drehen oder die Ebene zu
    /// wechseln. Atomar; ein Befehl ohne fertige Form hinterlässt bei Esc nichts.
    pub fn click3(&mut self, sk: &mut Sketch) -> Result<(), String> {
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
        if cur.origin {
            let pin = if flat {
                ConstraintKind::Fixed(p, 0.0, 0.0)
            } else {
                ConstraintKind::Fixed3(p, 0.0, 0.0, 0.0)
            };
            sk.add_constraint(pin).map_err(err)?;
        }
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

/// Eine ebene Form, die in 3D zeichnet (Spec 4): nur diese Arten, die übrigen sind bei sichtbarer
/// Triade gedämpft ([`crate::sketch_triad::kind_draws_3d`]).
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
}

impl Planar3 {
    /// Klicks bis zur fertigen Form.
    fn clicks(self) -> usize {
        match self {
            Planar3::Circle2P | Planar3::Rect2P => 2,
            Planar3::Arc3P | Planar3::Ellipse => 3,
            Planar3::Conic => 4,
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
        let at = plane.project(cur.at);
        let hit = Hit3 {
            at,
            snap: cur
                .snap
                .filter(|&q| sk.point_xyz(q).is_ok_and(|p| p.distance(at) <= tol)),
            origin: cur.origin && at.length() <= tol,
        };
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
                let center = new_point(sk, plane, plane.to3((a + b) / 2.0))?;
                Ok(Created::Circle(sk.add_circle3(center, r, n).map_err(err)?))
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

/// Die Meldung, wenn aus den Lagen keine ebene Form wird.
fn planar3_refusal(kind: Planar3) -> Msg {
    match kind {
        Planar3::Circle2P => crate::msg!("sketch-radius-positive"),
        Planar3::Arc3P => crate::msg!("sketch-arc-end-on-start"),
        Planar3::Rect2P => crate::msg!("sketch-err-rect-size"),
        Planar3::Ellipse => crate::msg!("sketch-major-axis-positive"),
        Planar3::Conic => crate::msg!("sketch-err-conic-apex"),
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
/// neuer; der Fang auf den Ursprung hält den neuen dort fest (`Fixed` bzw. `Fixed3`).
fn own_point(sk: &mut Sketch, plane: &Plane3, hit: &Hit3) -> Result<PointId, Msg> {
    if let Some(q) = hit.snap {
        return Ok(q);
    }
    let p = new_point(sk, plane, hit.at)?;
    if hit.origin {
        let pin = if plane.flat {
            ConstraintKind::Fixed(p, 0.0, 0.0)
        } else {
            ConstraintKind::Fixed3(p, 0.0, 0.0, 0.0)
        };
        sk.add_constraint(pin).map_err(err)?;
    }
    Ok(p)
}
