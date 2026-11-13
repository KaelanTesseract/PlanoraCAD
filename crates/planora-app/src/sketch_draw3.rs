//! Zeichnen in 3D (Skizzen 10, Task 11): Linie, Spline (beide Arten) und Punkt setzen ihre Punkte
//! in der aktiven Ebene der Triade ([`triad_point`]), mit Achsfang, Z-Sperre (Pfeil hoch), Fang auf
//! vorhandene Punkte und den Ursprung über den Bildschirmabstand sowie den getippten Werten der
//! Felder am Zeiger ([`crate::sketch_input`]).
//!
//! Der 3D-Weg gilt nur, solange die Triade nicht die Skizzenebene ist oder die Z-Sperre gehalten
//! wird ([`SketchSession::draws_in_3d`]). Auf der Skizzenebene zeichnen die Werkzeuge wie in 2D,
//! mit Fang, Ausrichtung und Raster; die Triade springt dort ebenso auf jeden gesetzten Punkt.

use crate::msg::Msg;
use crate::sketch_input::Created;
use crate::sketch_state::{SketchSession, Tool, err};
use crate::sketch_triad::{AXIS_SNAP_PX, TriadAxis, triad_point};
use crate::sketch_view::{Mapper, PICK_RADIUS_PX};
use egui::Pos2;
use glam::Vec2;
use planora_math::DVec3;
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, MAX_SPLINE_POINTS, PointId, Sketch};
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
    pub fn draws_in_3d(&self, sk: &Sketch) -> bool {
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
        self.with_open_start(sk, |se, sk| {
            se.atomic(sk, |se, sk, prior| se.click3_tool(sk, prior, &cur))
        })
        .map(|_| ())
    }

    /// Setzt mit dem aktiven Werkzeug (Linie, Spline, Punkt) einen Punkt an `p` (Skizzen-
    /// koordinaten), ohne Fang; `axis`: die Triadenachse, auf der `p` vom vorigen Punkt aus
    /// liegt. Liegt `p` in der Skizzenebene und ist die Triade die Skizzenebene, entsteht ein
    /// 2D-Punkt, sonst ein 3D-Punkt (Spec 1). Der Achsfang ergibt an einer Linie `Horizontal`,
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

    /// Der Punkt für den Klick: der gefangene oder ein neuer (2D nur auf der Skizzenebene bei
    /// unverdrehter Triade). Fang auf den Ursprung hält ihn dort fest: `Fixed` an einem 2D-,
    /// `Fixed3(0, 0, 0)` an einem 3D-Punkt.
    fn point3_at(&mut self, sk: &mut Sketch, cur: &Cursor3) -> Result<PointId, Msg> {
        if let Some(q) = cur.snap {
            return Ok(q);
        }
        let flat = cur.at.z.abs() <= sk.tolerance().linear && self.triad.is_sketch_plane();
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
