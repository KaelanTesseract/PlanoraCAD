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

/// Prüft, dass jedes Loch vollständig in der äußeren Schleife liegt, keine zwei Schleifen
/// sich schneiden oder berühren und kein Loch in einem anderen Loch liegt.
///
/// Arbeitet auf [`SampledLoop`]s mit `LAYOUT_STEPS_PER_TURN` Schritten je Vollkreis. Zwei
/// Schleifen gelten als sich berührend, wenn ihre Abtastungen sich näher kommen als
/// `tol.linear` plus die Pfeilhöhen der beteiligten Sehnen (siehe [`SampledLoop::touches`]).
/// Ein Loch, das die äußere Schleife tangential berührt, wird damit unabhängig von Rundung und
/// Maßstab abgelehnt.
pub(crate) fn check_hole_layout(profile: &Profile, tol: &Tolerance) -> Result<(), KernelError> {
    let outer = SampledLoop::new(&profile.outer, LAYOUT_STEPS_PER_TURN, tol);
    let holes: Vec<SampledLoop> = profile
        .holes
        .iter()
        .map(|h| SampledLoop::new(h, LAYOUT_STEPS_PER_TURN, tol))
        .collect();
    for (i, h) in holes.iter().enumerate() {
        if !outer.boxes_near(h, tol.linear) {
            return Err(KernelError::HoleOutsideOuter);
        }
        if outer.touches(h, tol.linear) {
            return Err(KernelError::ProfileLoopsIntersect);
        }
        if !outer.contains(h.points[0]) {
            return Err(KernelError::HoleOutsideOuter);
        }
        for other in &holes[i + 1..] {
            if !h.boxes_near(other, tol.linear) {
                continue;
            }
            if h.touches(other, tol.linear) {
                return Err(KernelError::ProfileLoopsIntersect);
            }
            // Ohne Berührung liegt entweder eine Schleife ganz in der anderen oder keine.
            if other.contains(h.points[0]) || h.contains(other.points[0]) {
                return Err(KernelError::NestedHoles);
            }
        }
    }
    Ok(())
}

/// Geschlossen gedachter Linienzug einer abgetasteten Schleife mit der Fehlerschranke jeder
/// Sehne. Gemeinsame Grundlage der Schnitt-, Berühr- und Enthaltensein-Prüfungen von Kernel
/// und Skizze (`planora-sketch` benutzt sie für `Sketch::profiles`).
///
/// Bögen werden durch Sehnen ersetzt; eine Sehne über den Winkel `φ` eines Bogens mit Radius
/// `r` weicht höchstens um die Pfeilhöhe `r·(1 − cos(φ/2))` vom Bogen ab (bei 64 Schritten je
/// Vollkreis etwa 0,12 % des Radius). Die Prüfungen rechnen diese Abweichung als Spielraum
/// hinzu, damit eine echte Berührung (Abstand 0) immer erkannt wird, egal ob ein Abtastpunkt
/// genau auf der Berührstelle liegt und unabhängig vom Maßstab.
///
/// Grenze: Echte Abstände unter `tol.linear` plus den Pfeilhöhen der beiden nächsten Sehnen
/// gelten als Berührung, bei Bögen also etwa unter 0,12 % des Radius je beteiligtem Bogen
/// (Radius 10 mm: etwa 0,012 mm).
#[derive(Clone, Debug)]
pub struct SampledLoop {
    /// Abgetastete Punkte, geschlossen gedacht (der letzte ist nicht der erste).
    pub points: Vec<DVec2>,
    /// Pfeilhöhe der Sehne `i` (von `points[i]` nach `points[(i + 1) % n]`); 0 bei Linien.
    pub sagitta: Vec<f64>,
    /// Hüllrechteck `(min, max)` der Punkte.
    pub bbox: (DVec2, DVec2),
}

impl SampledLoop {
    /// Tastet `lp` mit `steps_per_turn` Schritten je Vollkreis ab (dieselben Punkte wie
    /// [`ProfileLoop::sample`]). Endet die Abtastung höchstens `tol.linear` vom Start entfernt,
    /// entfällt der doppelte Endpunkt.
    pub fn new(lp: &ProfileLoop, steps_per_turn: usize, tol: &Tolerance) -> SampledLoop {
        let steps_per_turn = steps_per_turn.clamp(1, MAX_SAMPLES_PER_ARC);
        let mut points = vec![lp.start];
        // Pfeilhöhe der Sehne, die beim jeweiligen Punkt endet.
        let mut into = vec![0.0];
        let mut p = lp.start;
        for s in &lp.segments {
            match s {
                Segment::Line { to } => {
                    points.push(*to);
                    into.push(0.0);
                }
                Segment::Arc { center, sweep } => {
                    // dieselbe Schrittzahl wie `ProfileLoop::sample`
                    let n = ((sweep.abs() / TAU) * steps_per_turn as f64)
                        .ceil()
                        .max(2.0)
                        .min(MAX_SAMPLES_PER_ARC as f64) as usize;
                    let r = (p - *center).length();
                    let sag = r * (1.0 - (0.5 * sweep.abs() / n as f64).cos());
                    for k in 1..=n {
                        points.push(*center + rotate(p - *center, sweep * k as f64 / n as f64));
                        into.push(sag);
                    }
                }
            }
            p = segment_end(p, s);
        }
        // Liegt der letzte Punkt auf dem ersten, ist die Schlusssehne die letzte echte Sehne.
        if points.len() > 1 && points[points.len() - 1].distance(points[0]) <= tol.linear {
            points.pop();
            into[0] = into.pop().unwrap_or(0.0);
        }
        let n = points.len();
        // `into[k]` gehört zur Sehne `k − 1 → k`, die Sehne `i` also zu `into[i + 1]`.
        let sagitta = (0..n).map(|i| into[(i + 1) % n]).collect();
        let bbox = points.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), q| (lo.min(*q), hi.max(*q)),
        );
        SampledLoop {
            points,
            sagitta,
            bbox,
        }
    }

    /// Größte Pfeilhöhe aller Sehnen.
    pub fn max_sagitta(&self) -> f64 {
        self.sagitta.iter().copied().fold(0.0, f64::max)
    }

    fn segment(&self, i: usize) -> (DVec2, DVec2) {
        (self.points[i], self.points[(i + 1) % self.points.len()])
    }

    /// Können sich die Schleifen nach ihren um den Spielraum vergrößerten Hüllrechtecken
    /// überhaupt berühren?
    pub fn boxes_near(&self, other: &SampledLoop, linear: f64) -> bool {
        let m = linear + self.max_sagitta() + other.max_sagitta();
        self.bbox.0.x - m <= other.bbox.1.x
            && other.bbox.0.x - m <= self.bbox.1.x
            && self.bbox.0.y - m <= other.bbox.1.y
            && other.bbox.0.y - m <= self.bbox.1.y
    }

    /// Schneiden oder berühren sich die beiden Schleifen? Zwei Sehnen gelten als berührend, wenn
    /// ihr Abstand höchstens `linear` plus ihre beiden Pfeilhöhen beträgt.
    pub fn touches(&self, other: &SampledLoop, linear: f64) -> bool {
        if !self.boxes_near(other, linear) {
            return false;
        }
        (0..self.points.len()).any(|i| {
            let (p, q) = self.segment(i);
            (0..other.points.len()).any(|j| {
                let (r, s) = other.segment(j);
                segment_distance(p, q, r, s) <= linear + self.sagitta[i] + other.sagitta[j]
            })
        })
    }

    /// Kommen sich zwei nicht benachbarte Sehnen derselben Schleife näher als `linear` plus
    /// ihre Pfeilhöhen (Selbstschnitt oder Selbstberührung)?
    pub fn touches_itself(&self, linear: f64) -> bool {
        let n = self.points.len();
        if n < 4 {
            // Mit höchstens drei Sehnen ist jedes Paar benachbart.
            return false;
        }
        for i in 0..n {
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue; // benachbart über den Schluss
                }
                let (p, q) = self.segment(i);
                let (r, s) = self.segment(j);
                if segment_distance(p, q, r, s) <= linear + self.sagitta[i] + self.sagitta[j] {
                    return true;
                }
            }
        }
        false
    }

    /// Liegt `p` im Inneren des Linienzugs (Strahlverfahren, gerade/ungerade Schnittzahl)?
    /// Eindeutig für Punkte, die weiter als der Spielraum vom Rand entfernt sind.
    pub fn contains(&self, p: DVec2) -> bool {
        point_in_polygon(p, &self.points)
    }
}

/// Abstand von `p` zur Strecke `a`–`b`.
fn point_segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(d) / len2).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

/// Abstand der Strecken `a`–`b` und `c`–`d` (0, wenn sie sich schneiden).
fn segment_distance(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> f64 {
    if segments_touch(a, b, c, d) {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
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

/// Strahlverfahren (gerade/ungerade Anzahl Schnitte); das Polygon gilt als geschlossen.
fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}
