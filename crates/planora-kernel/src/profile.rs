use crate::error::KernelError;
use planora_math::{DVec2, TAU, Tolerance, orient2d};

/// Obergrenze der Abtastschritte je Bogen (verhindert Hängen und riesige Allokationen
/// bei sehr großen oder nicht endlichen Öffnungswinkeln).
pub const MAX_SAMPLES_PER_ARC: usize = 65_536;

/// Abtastung je Vollkreis für die Lageprüfung der Löcher (wie bei `signed_area`).
const LAYOUT_STEPS_PER_TURN: usize = 64;

/// Profilsegment. Der Startpunkt ist immer das Ende des vorigen Segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Line {
        to: DVec2,
    },
    /// Kreisbogen um `center`; `sweep` in Radiant, positiv = gegen den Uhrzeigersinn.
    Arc {
        center: DVec2,
        sweep: f64,
    },
}

fn rotate(v: DVec2, angle: f64) -> DVec2 {
    let (s, c) = angle.sin_cos();
    DVec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// Endpunkt eines Segments, das bei `from` beginnt.
pub fn segment_end(from: DVec2, seg: &Segment) -> DVec2 {
    match seg {
        Segment::Line { to } => *to,
        Segment::Arc { center, sweep } => *center + rotate(from - *center, *sweep),
    }
}

/// Folge von Segmenten. Als Schleife (Extrusion) muss sie geschlossen sein,
/// als Kette (Rotation) darf sie offen sein.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileLoop {
    pub start: DVec2,
    pub segments: Vec<Segment>,
}

impl ProfileLoop {
    /// Geschlossenes Polygon durch die gegebenen Punkte.
    ///
    /// Fehler `DegenerateSegment` bei weniger als drei Punkten oder nicht endlichen Werten.
    pub fn polygon(points: &[DVec2]) -> Result<ProfileLoop, KernelError> {
        if points.len() < 3 || points.iter().any(|p| !p.is_finite()) {
            return Err(KernelError::DegenerateSegment);
        }
        Ok(Self::closed_lines(points))
    }

    /// Baut das geschlossene Polygon ohne Prüfung (mindestens ein Punkt nötig).
    fn closed_lines(points: &[DVec2]) -> ProfileLoop {
        ProfileLoop {
            start: points[0],
            segments: points[1..]
                .iter()
                .map(|p| Segment::Line { to: *p })
                .chain(std::iter::once(Segment::Line { to: points[0] }))
                .collect(),
        }
    }

    /// Achsparalleles Rechteck. Unfehlbar: ungültige Werte fallen erst bei der Extrusion auf.
    pub fn rectangle(min: DVec2, max: DVec2) -> ProfileLoop {
        Self::closed_lines(&[min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)])
    }

    /// Vollkreis gegen den Uhrzeigersinn, beginnend bei `center + (radius, 0)`.
    pub fn circle(center: DVec2, radius: f64) -> ProfileLoop {
        ProfileLoop {
            start: center + DVec2::new(radius, 0.0),
            segments: vec![Segment::Arc { center, sweep: TAU }],
        }
    }

    /// Alle Segmentgrenzen: Startpunkt und das Ende jedes Segments (`segments.len() + 1` Punkte).
    pub fn chain_points(&self) -> Vec<DVec2> {
        let mut pts = vec![self.start];
        let mut p = self.start;
        for s in &self.segments {
            p = segment_end(p, s);
            pts.push(p);
        }
        pts
    }

    pub fn is_closed(&self, tol: &Tolerance) -> bool {
        self.chain_points()
            .last()
            .is_some_and(|end| end.distance(self.start) <= tol.linear)
    }

    /// Punktfolge entlang der Kurve; Bögen werden mit `steps_per_turn` Schritten pro Vollkreis genähert.
    ///
    /// `steps_per_turn` wird auf `1..=MAX_SAMPLES_PER_ARC` begrenzt, jeder Bogen erhält
    /// zwischen 2 und `MAX_SAMPLES_PER_ARC` Schritte (auch bei nicht endlichem Winkel).
    pub fn sample(&self, steps_per_turn: usize) -> Vec<DVec2> {
        let steps_per_turn = steps_per_turn.clamp(1, MAX_SAMPLES_PER_ARC);
        let mut out = vec![self.start];
        let mut p = self.start;
        for s in &self.segments {
            match s {
                Segment::Line { to } => out.push(*to),
                Segment::Arc { center, sweep } => {
                    // `f64::max(NaN, 2.0)` ist 2.0; `min` begrenzt unendliche Werte.
                    let n = ((sweep.abs() / TAU) * steps_per_turn as f64)
                        .ceil()
                        .max(2.0)
                        .min(MAX_SAMPLES_PER_ARC as f64) as usize;
                    for k in 1..=n {
                        out.push(*center + rotate(p - *center, sweep * k as f64 / n as f64));
                    }
                }
            }
            p = segment_end(p, s);
        }
        out
    }

    /// Vorzeichenbehaftete Fläche (positiv = gegen den Uhrzeigersinn). Offene Ketten
    /// werden dabei gedanklich geradlinig geschlossen.
    pub fn signed_area(&self) -> f64 {
        let pts = self.sample(64);
        let n = pts.len();
        (0..n)
            .map(|i| pts[i].perp_dot(pts[(i + 1) % n]))
            .sum::<f64>()
            / 2.0
    }

    /// Dieselbe Kurve in umgekehrter Richtung.
    pub fn reversed(&self) -> ProfileLoop {
        let pts = self.chain_points();
        let m = self.segments.len();
        let segments = (0..m)
            .rev()
            .map(|i| match self.segments[i] {
                Segment::Line { .. } => Segment::Line { to: pts[i] },
                Segment::Arc { center, sweep } => Segment::Arc {
                    center,
                    sweep: -sweep,
                },
            })
            .collect();
        ProfileLoop {
            start: pts[m],
            segments,
        }
    }
}

/// Ebenes Profil: eine äußere Schleife und beliebig viele Löcher.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub outer: ProfileLoop,
    pub holes: Vec<ProfileLoop>,
}

impl Profile {
    /// Äußere Schleife gegen, Löcher im Uhrzeigersinn.
    pub fn oriented(&self) -> Profile {
        let outer = if self.outer.signed_area() < 0.0 {
            self.outer.reversed()
        } else {
            self.outer.clone()
        };
        let holes = self
            .holes
            .iter()
            .map(|h| {
                if h.signed_area() > 0.0 {
                    h.reversed()
                } else {
                    h.clone()
                }
            })
            .collect();
        Profile { outer, holes }
    }
}

/// Prüft eine Schleife, bevor sie abgetastet wird: nicht leer, alle Werte endlich,
/// jeder Bogen mit Öffnungswinkel zwischen `tol.angular` und einer Vollkreisdrehung.
pub(crate) fn check_loop_input(lp: &ProfileLoop, tol: &Tolerance) -> Result<(), KernelError> {
    if lp.segments.is_empty() || !lp.start.is_finite() {
        return Err(KernelError::DegenerateSegment);
    }
    for seg in &lp.segments {
        let ok = match seg {
            Segment::Line { to } => to.is_finite(),
            Segment::Arc { center, sweep } => {
                center.is_finite()
                    && sweep.is_finite()
                    && sweep.abs() > tol.angular
                    && sweep.abs() <= TAU + 1e-9
            }
        };
        if !ok {
            return Err(KernelError::DegenerateSegment);
        }
    }
    if lp.chain_points().iter().any(|p| !p.is_finite()) {
        return Err(KernelError::DegenerateSegment);
    }
    Ok(())
}

/// Prüft, dass jedes Loch vollständig in der äußeren Schleife liegt und keine zwei Schleifen
/// sich schneiden oder berühren. Arbeitet auf den mit `LAYOUT_STEPS_PER_TURN` Schritten je
/// Vollkreis abgetasteten Linienzügen; Berührung zählt als Schnitt.
pub(crate) fn check_hole_layout(profile: &Profile) -> Result<(), KernelError> {
    let outer = profile.outer.sample(LAYOUT_STEPS_PER_TURN);
    let holes: Vec<Vec<DVec2>> = profile
        .holes
        .iter()
        .map(|h| h.sample(LAYOUT_STEPS_PER_TURN))
        .collect();
    for (i, h) in holes.iter().enumerate() {
        if polylines_touch(&outer, h) {
            return Err(KernelError::ProfileLoopsIntersect);
        }
        if !point_in_polygon(h[0], &outer) {
            return Err(KernelError::HoleOutsideOuter);
        }
        for other in &holes[i + 1..] {
            if polylines_touch(h, other) {
                return Err(KernelError::ProfileLoopsIntersect);
            }
        }
    }
    Ok(())
}

/// Segmente eines geschlossen betrachteten Linienzugs (letzter Punkt zurück zum ersten).
fn closed_segments(pts: &[DVec2]) -> impl Iterator<Item = (DVec2, DVec2)> + '_ {
    let n = pts.len();
    (0..n).map(move |i| (pts[i], pts[(i + 1) % n]))
}

/// Liegt `p` (kollinear zu `a`–`b`) im achsparallelen Bereich der Strecke?
fn within_range(a: DVec2, b: DVec2, p: DVec2) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Schneiden oder berühren sich die Strecken `a`–`b` und `c`–`d`? Exakter Vorzeichentest.
fn segments_touch(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = orient2d(c, d, a);
    let d2 = orient2d(c, d, b);
    let d3 = orient2d(a, b, c);
    let d4 = orient2d(a, b, d);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && within_range(c, d, a))
        || (d2 == 0.0 && within_range(c, d, b))
        || (d3 == 0.0 && within_range(a, b, c))
        || (d4 == 0.0 && within_range(a, b, d))
}

/// Schneidet oder berührt irgendein Segment von `a` irgendeines von `b` (beide geschlossen)?
fn polylines_touch(a: &[DVec2], b: &[DVec2]) -> bool {
    closed_segments(a).any(|(p, q)| closed_segments(b).any(|(r, s)| segments_touch(p, q, r, s)))
}

/// Strahlverfahren (gerade/ungerade Anzahl Schnitte); das Polygon gilt als geschlossen.
fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    for (a, b) in closed_segments(poly) {
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}
