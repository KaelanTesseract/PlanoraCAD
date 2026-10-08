use crate::error::KernelError;
use planora_math::{DVec2, TAU, Tolerance};

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
    pub fn polygon(points: &[DVec2]) -> ProfileLoop {
        assert!(
            points.len() >= 3,
            "ein Polygon braucht mindestens drei Punkte"
        );
        ProfileLoop {
            start: points[0],
            segments: points[1..]
                .iter()
                .map(|p| Segment::Line { to: *p })
                .chain(std::iter::once(Segment::Line { to: points[0] }))
                .collect(),
        }
    }

    pub fn rectangle(min: DVec2, max: DVec2) -> ProfileLoop {
        ProfileLoop::polygon(&[min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)])
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
    pub fn sample(&self, steps_per_turn: usize) -> Vec<DVec2> {
        let mut out = vec![self.start];
        let mut p = self.start;
        for s in &self.segments {
            match s {
                Segment::Line { to } => out.push(*to),
                Segment::Arc { center, sweep } => {
                    let n = ((sweep.abs() / TAU) * steps_per_turn as f64)
                        .ceil()
                        .max(2.0) as usize;
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
