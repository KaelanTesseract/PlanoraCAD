//! Fang-Index einer Skizze: Punkte, Mitten, Schnittpunkte, Kurven und Ursprung als
//! Fangziele sowie Ausrichtung an den Koordinaten vorhandener Punkte. Der Index wird je
//! Skizzenstand einmal gebaut ([`SnapIndex::build`]) und je Bild nur befragt.

use crate::geom::Geom;
use crate::intersect::{Prepared, intersect_prepared};
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_math::DVec2;
use std::collections::HashSet;

/// Schritte je Vollkreis des Linienzugs, den die Vorauswahl der Kurvenlage benutzt.
const SNAP_STEPS_PER_TURN: usize = 96;
/// Zuschlag zur Vorauswahl über dem Linienzug, relativ zur Größe der Hüllbox (Sehnenfehler).
const POLY_SLACK: f64 = 2e-3;

/// Art des Fangs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    None,
    Point(PointId),
    /// Punkt, der nur Mitte eines Kreises, Bogens, einer Ellipse oder eines Ellipsenbogens ist.
    Center(PointId),
    Midpoint(CurveId),
    Intersection(CurveId, CurveId),
    Curve(CurveId),
    Origin,
    Grid,
}

/// Gefangene Art und Lage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapHit {
    pub snap: Snap,
    pub at: DVec2,
}

/// Ausrichtung: `x` rastet den Zeiger auf das x des Punktes (senkrechte Hilfslinie), `y`
/// auf das y (waagerechte). Der Wert ist die Koordinate, auf die einzurasten ist.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AlignHit {
    pub x: Option<(PointId, f64)>,
    pub y: Option<(PointId, f64)>,
}

#[derive(Clone, Debug)]
struct IndexedCurve {
    id: CurveId,
    geom: Geom,
    bbox: (DVec2, DVec2),
    poly: Vec<DVec2>,
}

/// Fangziele einer Skizze zu einem Stand ([`SnapIndex::revision`]).
#[derive(Clone, Debug)]
pub struct SnapIndex {
    revision: u64,
    /// Punkte mit Lage und Art (`Point` oder `Center`), nach Id aufsteigend.
    points: Vec<(Snap, DVec2)>,
    mids: Vec<(CurveId, DVec2)>,
    crossings: Vec<(CurveId, CurveId, DVec2)>,
    curves: Vec<IndexedCurve>,
    origin_free: bool,
}

/// Merkt sich das beste Ziel: kleinerer Abstand gewinnt, bei Gleichstand das frühere
/// (die Ziele werden nach Id aufsteigend angeboten).
fn take(best: &mut Option<(f64, SnapHit)>, radius: f64, d: f64, snap: Snap, at: DVec2) {
    if d <= radius && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
        *best = Some((d, SnapHit { snap, at }));
    }
}

impl SnapIndex {
    pub fn build(sk: &Sketch) -> SnapIndex {
        let tol = sk.tolerance().linear;
        // Rollen der Punkte: nur Mitte ⇒ Center
        let mut other_role: HashSet<usize> = HashSet::new();
        let mut centers: HashSet<usize> = HashSet::new();
        let mut curves = Vec::new();
        let mut mids = Vec::new();
        for (id, data) in sk.curves() {
            match &data.kind {
                CurveKind::Line { a, b } => {
                    other_role.insert(a.index());
                    other_role.insert(b.index());
                }
                CurveKind::Circle { center, .. } | CurveKind::Ellipse { center, .. } => {
                    centers.insert(center.index());
                }
                CurveKind::Arc { center, start, end }
                | CurveKind::EllipticArc {
                    center, start, end, ..
                } => {
                    centers.insert(center.index());
                    other_role.insert(start.index());
                    other_role.insert(end.index());
                }
                CurveKind::Spline { points } => {
                    other_role.extend(points.iter().map(|p| p.index()));
                }
            }
            let Some(geom) = sk.geom(id) else { continue };
            if matches!(geom, Geom::Line { .. } | Geom::Arc { .. }) {
                // Die Domäne ist absolut (Bögen laufen von a0 bis a0 + sweep).
                let (lo, hi) = geom.domain();
                let m = geom.eval(0.5 * (lo + hi)).0;
                if m.is_finite() {
                    mids.push((id, m));
                }
            }
            let bbox = geom.bbox();
            if bbox.0.is_finite() && bbox.1.is_finite() {
                let poly = sk
                    .curve_polyline(id, SNAP_STEPS_PER_TURN)
                    .unwrap_or_default();
                curves.push(IndexedCurve {
                    id,
                    geom,
                    bbox,
                    poly,
                });
            }
        }
        let mut points = Vec::new();
        let mut origin_free = true;
        for (id, _) in sk.points() {
            let p = sk.pos(id);
            if !p.is_finite() {
                continue;
            }
            if p.length() <= tol {
                origin_free = false;
            }
            let snap = if centers.contains(&id.index()) && !other_role.contains(&id.index()) {
                Snap::Center(id)
            } else {
                Snap::Point(id)
            };
            points.push((snap, p));
        }
        let prepared: Vec<Prepared> = curves.iter().map(|c| Prepared::new(&c.geom)).collect();
        let mut crossings = Vec::new();
        for i in 0..curves.len() {
            for j in (i + 1)..curves.len() {
                for (t1, _) in intersect_prepared(&prepared[i], &prepared[j], tol) {
                    let p = curves[i].geom.eval(t1).0;
                    if p.is_finite() {
                        crossings.push((curves[i].id, curves[j].id, p));
                    }
                }
            }
        }
        SnapIndex {
            revision: sk.revision(),
            points,
            mids,
            crossings,
            curves,
            origin_free,
        }
    }

    /// Stand der Skizze, aus dem der Index gebaut wurde.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Bestes Fangziel im Radius um `at`. Die Klasse entscheidet vor dem Abstand: Punkt,
    /// Mitte und Ursprung vor Mittelpunkt vor Schnittpunkt vor Kurve; innerhalb der Klasse
    /// der nächste, bei gleichem Abstand die kleinere Id.
    pub fn find(&self, at: DVec2, radius: f64) -> Option<SnapHit> {
        self.find_avoiding(at, radius, &[], 0.0)
    }

    /// Wie [`find`](Self::find), aber Ziele, die höchstens `tol` neben einer Lage aus `avoid`
    /// liegen, zählen nicht (die offenen Punkte des laufenden Befehls: ein Klick dorthin ergäbe
    /// eine Form ohne Größe).
    pub fn find_avoiding(
        &self,
        at: DVec2,
        radius: f64,
        avoid: &[DVec2],
        tol: f64,
    ) -> Option<SnapHit> {
        if !at.is_finite() {
            return None;
        }
        let radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        };
        let mut best: Option<(f64, SnapHit)> = None;
        let take = |best: &mut Option<(f64, SnapHit)>, radius, d, snap, p: DVec2| {
            if !avoid.iter().any(|a| a.distance(p) <= tol) {
                take(best, radius, d, snap, p);
            }
        };

        for &(snap, p) in &self.points {
            take(&mut best, radius, p.distance(at), snap, p);
        }
        if self.origin_free {
            take(&mut best, radius, at.length(), Snap::Origin, DVec2::ZERO);
        }
        if best.is_some() {
            return best.map(|(_, hit)| hit);
        }

        for &(c, p) in &self.mids {
            take(&mut best, radius, p.distance(at), Snap::Midpoint(c), p);
        }
        if best.is_some() {
            return best.map(|(_, hit)| hit);
        }

        for &(a, b, p) in &self.crossings {
            take(
                &mut best,
                radius,
                p.distance(at),
                Snap::Intersection(a, b),
                p,
            );
        }
        if best.is_some() {
            return best.map(|(_, hit)| hit);
        }

        // Kurven: Vorauswahl über Hüllbox und Linienzug, Lage über den nächsten Kurvenpunkt
        // nachgeschärft.
        for c in &self.curves {
            let slack = POLY_SLACK * (c.bbox.1 - c.bbox.0).max_element().max(0.0);
            let reach = radius + slack;
            if at.x < c.bbox.0.x - reach
                || at.x > c.bbox.1.x + reach
                || at.y < c.bbox.0.y - reach
                || at.y > c.bbox.1.y + reach
            {
                continue;
            }
            let near = c
                .poly
                .windows(2)
                .map(|w| segment_distance(at, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            if near > reach {
                continue;
            }
            let p = c.geom.eval(c.geom.nearest(at)).0;
            if p.is_finite() {
                take(&mut best, radius, p.distance(at), Snap::Curve(c.id), p);
            }
        }
        best.map(|(_, hit)| hit)
    }

    /// Ausrichtung an den Koordinaten lebender Punkte außer `skip` (nur innerhalb von
    /// `radius`; der nächste gewinnt, bei Gleichstand die kleinere Id).
    pub fn align(&self, at: DVec2, radius: f64, skip: &[PointId]) -> AlignHit {
        let mut hit = AlignHit::default();
        if !at.is_finite() {
            return hit;
        }
        let radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        };
        let (mut bx, mut by) = (f64::INFINITY, f64::INFINITY);
        for &(snap, p) in &self.points {
            let (Snap::Point(id) | Snap::Center(id)) = snap else {
                continue;
            };
            if skip.contains(&id) {
                continue;
            }
            let dx = (p.x - at.x).abs();
            if dx <= radius && dx < bx {
                bx = dx;
                hit.x = Some((id, p.x));
            }
            let dy = (p.y - at.y).abs();
            if dy <= radius && dy < by {
                by = dy;
                hit.y = Some((id, p.y));
            }
        }
        hit
    }
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let len2 = d.length_squared();
    let t = if len2 > 0.0 {
        (p - a).dot(d) / len2
    } else {
        0.0
    };
    p.distance(a + d * t.clamp(0.0, 1.0))
}
