//! Darstellungs- und Auswahlhilfen: Linienzüge der Kurven, Fangen von Punkten und Kurven,
//! Hülle der Skizze.

use crate::curves::Sampled;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_math::{DVec2, TAU};

/// Kleinste und größte Schrittzahl je Vollkreis für [`Sketch::curve_polyline`].
const MIN_STEPS_PER_TURN: usize = 4;
const MAX_STEPS_PER_TURN: usize = 4096;
/// Punkte je Spline-Segment im Linienzug.
const SPLINE_STEPS_PER_SEGMENT: usize = 16;
/// Schritte je Vollkreis für Abstand und Fangen (fein genug für die Bildschirmauswahl).
const PICK_STEPS_PER_TURN: usize = 96;
/// Schritte je Vollkreis für [`Sketch::extent`] (Fehler unter 0,2 % des Radius, genügt zum
/// Einpassen der Ansicht).
const EXTENT_STEPS_PER_TURN: usize = 64;

/// Abstand des Punktes `p` von der Strecke `a`–`b`.
fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    p.distance(segment_closest(p, a, b))
}

/// Nächster Punkt der Strecke `a`–`b` zu `p`.
fn segment_closest(p: DVec2, a: DVec2, b: DVec2) -> DVec2 {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 == 0.0 {
        return a;
    }
    let t = ((p - a).dot(d) / len2).clamp(0.0, 1.0);
    a + d * t
}

/// Nächster Punkt eines Linienzugs zu `p` (`None` für leere Linienzüge).
fn polyline_closest(pl: &[DVec2], p: DVec2) -> Option<DVec2> {
    if pl.len() == 1 {
        return Some(pl[0]);
    }
    pl.windows(2)
        .map(|w| segment_closest(p, w[0], w[1]))
        .min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)))
}

/// Punkte eines Kreisbogens um `c` vom Winkel `a0` über `sweep` mit Radius `r`; `steps`
/// Abschnitte. Der erste und der letzte Punkt werden vom Aufrufer gesetzt.
fn arc_points(c: DVec2, r: f64, a0: f64, sweep: f64, steps: usize) -> Vec<DVec2> {
    (0..=steps)
        .map(|k| {
            let a = a0 + sweep * k as f64 / steps as f64;
            c + DVec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Abschnitte für den Winkel `sweep` bei `per_turn` Abschnitten je Vollkreis (mindestens 1).
fn steps_for(sweep: f64, per_turn: usize) -> usize {
    ((sweep.abs() / TAU * per_turn as f64).ceil() as usize).max(1)
}

impl Sketch {
    /// Linienzug einer lebenden Kurve: Linie 2 Punkte; Kreis und Ellipse geschlossen (letzter
    /// Punkt = erster); Bogen gegen den Uhrzeigersinn von Start nach Ende mit den exakten
    /// Endpunkten; Spline 16 Punkte je Segment. `steps_per_turn` wird auf `4..=4096` geklemmt.
    pub fn curve_polyline(&self, c: CurveId, steps_per_turn: usize) -> Option<Vec<DVec2>> {
        let per_turn = steps_per_turn.clamp(MIN_STEPS_PER_TURN, MAX_STEPS_PER_TURN);
        let tol = self.tolerance();
        let pl = match &self.curve(c)?.kind {
            CurveKind::Line { a, b } => vec![self.pos(*a), self.pos(*b)],
            CurveKind::Circle { center, radius } => {
                let mut pl = arc_points(self.pos(*center), self.vars[*radius], 0.0, TAU, per_turn);
                pl[per_turn] = pl[0];
                pl
            }
            CurveKind::Arc { center, start, end } => {
                let (m, s, e) = (self.pos(*center), self.pos(*start), self.pos(*end));
                let a0 = (s - m).y.atan2((s - m).x);
                let a1 = (e - m).y.atan2((e - m).x);
                let mut sweep = (a1 - a0).rem_euclid(TAU);
                // Start und Ende in derselben Richtung: voller Umlauf.
                if sweep <= tol.angular {
                    sweep = TAU;
                }
                let steps = steps_for(sweep, per_turn);
                let mut pl = arc_points(m, s.distance(m), a0, sweep, steps);
                pl[0] = s;
                pl[steps] = e;
                pl
            }
            CurveKind::Ellipse { .. } => {
                let mut pl: Vec<DVec2> = (0..=per_turn)
                    .map(|k| self.ellipse_point(c, TAU * k as f64 / per_turn as f64))
                    .collect::<Option<_>>()?;
                pl[per_turn] = pl[0];
                pl
            }
            CurveKind::Spline { points } => {
                let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
                let sampled = Sampled::spline(&xy);
                let n = (xy.len() - 1) * SPLINE_STEPS_PER_SEGMENT;
                let mut pl: Vec<DVec2> = (0..=n)
                    .map(|k| sampled.eval(k as f64 / SPLINE_STEPS_PER_SEGMENT as f64).0)
                    .collect();
                pl[0] = xy[0];
                pl[n] = xy[xy.len() - 1];
                pl
            }
        };
        Some(pl)
    }

    /// Abstand von `at` zur lebenden Kurve `c` (Linienzug mit 96 Schritten je Vollkreis).
    pub fn distance_to_curve(&self, c: CurveId, at: DVec2) -> Option<f64> {
        let pl = self.curve_polyline(c, PICK_STEPS_PER_TURN)?;
        if pl.len() == 1 {
            return Some(pl[0].distance(at));
        }
        pl.windows(2)
            .map(|w| segment_distance(at, w[0], w[1]))
            .min_by(f64::total_cmp)
    }

    /// Punkt der lebenden Kurve `c`, der `at` am nächsten liegt: exakt für Linien (auf der
    /// unbegrenzten Geraden) sowie Kreise und Bögen (auf dem ganzen Kreis), für Ellipsen und
    /// Splines auf dem Linienzug.
    pub fn nearest_on_curve(&self, c: CurveId, at: DVec2) -> Option<DVec2> {
        if !at.is_finite() {
            return None;
        }
        let around = |m: DVec2, r: f64| {
            let d = at - m;
            if d.length() == 0.0 {
                m + DVec2::new(r, 0.0)
            } else {
                m + d.normalize() * r
            }
        };
        match &self.curve(c)?.kind {
            CurveKind::Line { a, b } => {
                let (pa, pb) = (self.pos(*a), self.pos(*b));
                let d = pb - pa;
                let len2 = d.length_squared();
                Some(if len2 == 0.0 {
                    pa
                } else {
                    pa + d * ((at - pa).dot(d) / len2)
                })
            }
            CurveKind::Circle { center, radius } => {
                Some(around(self.pos(*center), self.vars[*radius]))
            }
            CurveKind::Arc { center, start, .. } => {
                let m = self.pos(*center);
                Some(around(m, self.pos(*start).distance(m)))
            }
            CurveKind::Ellipse { .. } | CurveKind::Spline { .. } => {
                polyline_closest(&self.curve_polyline(c, MAX_STEPS_PER_TURN)?, at)
            }
        }
    }

    /// Nächster lebender Punkt innerhalb von `radius` um `at`.
    pub fn pick_point(&self, at: DVec2, radius: f64) -> Option<PointId> {
        self.points()
            .map(|(id, _)| (id, self.pos(id).distance(at)))
            .filter(|&(_, d)| d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// Nächste lebende Kurve innerhalb von `radius` um `at`.
    pub fn pick_curve(&self, at: DVec2, radius: f64) -> Option<CurveId> {
        self.curves()
            .filter_map(|(id, _)| Some((id, self.distance_to_curve(id, at)?)))
            .filter(|&(_, d)| d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// Achsenparallele Hülle `(min, max)` aller lebenden Punkte und Kurven (Kurven über ihren
    /// Linienzug); `None` für eine leere Skizze. Zum Einpassen der Ansicht.
    pub fn extent(&self) -> Option<(DVec2, DVec2)> {
        let curve_points = self
            .curves()
            .filter_map(|(id, _)| self.curve_polyline(id, EXTENT_STEPS_PER_TURN))
            .flatten();
        self.points()
            .map(|(id, _)| self.pos(id))
            .chain(curve_points)
            .filter(|p| p.is_finite())
            .fold(None, |acc, p| match acc {
                None => Some((p, p)),
                Some((lo, hi)) => Some((lo.min(p), hi.max(p))),
            })
    }

    /// Achsenparallele Hülle `(min, max)` aller lebenden Punkte; `None` ohne Punkte.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        self.points()
            .map(|(id, _)| self.pos(id))
            .fold(None, |acc, p| match acc {
                None => Some((p, p)),
                Some((lo, hi)) => Some((lo.min(p), hi.max(p))),
            })
    }
}
