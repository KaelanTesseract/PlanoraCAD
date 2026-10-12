use crate::error::KernelError;
use crate::geometry::nurbs_divisions;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, TAU, Tolerance, orient2d};
use std::sync::Arc;

/// Obergrenze der Abtastschritte je Bogen (verhindert Hängen und riesige Allokationen
/// bei sehr großen oder nicht endlichen Öffnungswinkeln).
pub const MAX_SAMPLES_PER_ARC: usize = 65_536;

/// Abtastung je Vollkreis für die Lageprüfung der Löcher (wie bei `signed_area`).
const LAYOUT_STEPS_PER_TURN: usize = 64;

/// Mindestzahl der Sehnen je Freiformsegment.
const MIN_NURBS_SAMPLES: usize = 64;
/// Sehnen je Kontrollpunkt eines Freiformsegments (Kurven mit vielen Kontrollpunkten werden
/// dadurch feiner abgetastet), begrenzt durch `MAX_NURBS_CHORDS`.
const NURBS_SAMPLES_PER_CONTROL: usize = 16;
/// Obergrenze der Sehnen je Freiformsegment. Die Pfeilhöhe jeder Sehne ist eine sichere Schranke
/// (siehe [`nurbs_chords`]); weniger Sehnen machen die Prüfungen nur vorsichtiger, nicht falsch.
pub const MAX_NURBS_CHORDS: usize = 4096;
/// Obergrenze aller Freiform-Sehnen eines Profils in der Lageprüfung (verteilt auf die
/// Freiformsegmente, jedes mindestens eine Sehne je Gruppe von Bézier-Stücken).
const PROFILE_NURBS_CHORD_BUDGET: usize = 1 << 16;

/// Profilsegment. Der Startpunkt ist immer das Ende des vorigen Segments.
///
/// Nicht `Copy` (wegen der geteilten Freiformkurve); Kopieren mit `clone` ist billig.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Line {
        to: DVec2,
    },
    /// Kreisbogen um `center`; `sweep` in Radiant, positiv = gegen den Uhrzeigersinn.
    Arc {
        center: DVec2,
        sweep: f64,
    },
    /// Freiformkurve in der xy-Ebene des Profils (`z = 0`, Abweichung höchstens `tol.linear`).
    /// Ihr Anfang muss innerhalb `tol.linear` am Ende des vorigen Segments liegen; das Ende des
    /// Segments ist das Kurvenende `curve.point(u_end)` (nur x und y).
    Nurbs {
        curve: Arc<NurbsCurve>,
    },
}

fn xy(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.y)
}

/// Zahl der gewünschten Sehnen eines Freiformsegments: mindestens `MIN_NURBS_SAMPLES`, mindestens
/// `NURBS_SAMPLES_PER_CONTROL` je Kontrollpunkt und mindestens so viele, wie die Form bei
/// `steps_per_turn` Schritten je Vollkreis verlangt ([`nurbs_divisions`]); höchstens `cap`.
fn nurbs_steps(c: &NurbsCurve, steps_per_turn: usize, cap: usize) -> usize {
    let (a, b) = c.domain();
    let by_shape = nurbs_divisions(c, a, b, TAU / steps_per_turn.max(1) as f64);
    MIN_NURBS_SAMPLES
        .max(NURBS_SAMPLES_PER_CONTROL.saturating_mul(c.control_points().len()))
        .max(by_shape)
        .min(cap.max(1))
}

/// Größter Abstand der Punkte (nur x und y) zur Strecke `a`–`b`.
fn hull_distance(points: &[DVec3], a: DVec2, b: DVec2) -> f64 {
    points
        .iter()
        .map(|p| point_segment_distance(xy(*p), a, b))
        .fold(0.0, f64::max)
}

/// Sehnen eines Freiformsegments, das bei `from` beginnt: Endpunkte (ohne `from`) und je Sehne
/// eine **sichere** Schranke ihrer Abweichung von der Kurve.
///
/// Die Kurve wird in ihre Bézier-Stücke zerlegt (je nichtleerem Knotenabschnitt eines, also auch
/// bei sehr ungleichmäßiger Parametrisierung) und die Stücke werden bei Bedarf halbiert, bis etwa
/// [`nurbs_steps`] Sehnen erreicht sind; gibt es mehr Stücke als erlaubt, überspannt eine Sehne
/// mehrere aufeinanderfolgende Stücke. Weil alle Gewichte positiv sind, liegt jedes Stück in der
/// konvexen Hülle seiner Kontrollpunkte; der Abstand zu einer Strecke ist konvex, sein Größtwert
/// über der Hülle wird also an einem Kontrollpunkt angenommen. Die Schranke einer Sehne ist darum
/// der größte Abstand der Kontrollpunkte ihrer Stücke von der Sehne.
fn nurbs_chords(
    c: &NurbsCurve,
    from: DVec2,
    steps_per_turn: usize,
    cap: usize,
) -> (Vec<DVec2>, Vec<f64>) {
    let target = nurbs_steps(c, steps_per_turn, cap);
    let mut pieces = c.bezier_segments();
    // Halbieren, solange das Ziel nicht erreicht ist (höchstens bis zum Ziel).
    while pieces.len() < target && pieces.len() * 2 <= target.max(2) {
        pieces = pieces.iter().flat_map(|p| p.split_half()).collect();
    }
    let group = pieces.len().div_ceil(target.max(1)).max(1);
    let end = xy(c.point(c.domain().1));
    let mut points = Vec::with_capacity(pieces.len() / group + 1);
    let mut sagitta = Vec::with_capacity(points.capacity());
    let mut prev = from;
    for (k, chunk) in pieces.chunks(group).enumerate() {
        let last = (k + 1) * group >= pieces.len();
        let q = if last {
            end
        } else {
            chunk.last().map_or(end, |p| xy(p.control[p.degree]))
        };
        let sag = chunk
            .iter()
            .map(|p| hull_distance(&p.control, prev, q))
            .fold(0.0, f64::max);
        points.push(q);
        sagitta.push(if sag.is_finite() { sag } else { f64::INFINITY });
        prev = q;
    }
    (points, sagitta)
}

/// Dieselbe Kurve in umgekehrter Richtung. Zuerst mit gespiegeltem Knotenvektor (gleicher
/// Parameterbereich); lässt Rundung dabei Knoten verschmelzen, mit negierten Knoten (Bereich
/// `[−b, −a]`). Das Negieren ist in Gleitkomma exakt, und alle Prüfungen von `KnotVector::new`
/// und `NurbsCurve::new` sind dagegen symmetrisch; für eine gültige Kurve scheitert es deshalb
/// nicht. Scheitert es doch, ist das ein Fehler (`InvalidCurve`).
fn reverse_curve(c: &NurbsCurve) -> Result<NurbsCurve, KernelError> {
    if let Ok(r) = c.reversed() {
        return Ok(r);
    }
    let knots: Vec<f64> = c.knots().knots().iter().rev().map(|k| -k).collect();
    let control: Vec<DVec3> = c.control_points().iter().rev().copied().collect();
    let weights: Vec<f64> = c.weights().iter().rev().copied().collect();
    NurbsCurve::new(c.degree(), knots, control, weights)
        .map_err(|e| KernelError::InvalidCurve(e.to_string()))
}

/// Legt den letzten Kontrollpunkt exakt auf den ersten, wenn Anfang und Ende der Kurve höchstens
/// `tol.linear` auseinanderliegen (geschlossene Kurve; die Form ändert sich höchstens um
/// `tol.linear`). Nötig, weil eine geschlossene Fläche (`periodic_u`) gleiche Randreihen verlangt.
pub(crate) fn snap_closed(c: &NurbsCurve, tol: &Tolerance) -> Result<NurbsCurve, KernelError> {
    let cp = c.control_points();
    let n = cp.len();
    if cp[0] == cp[n - 1] || xy(cp[0]).distance(xy(cp[n - 1])) > tol.linear {
        return Ok(c.clone());
    }
    let mut control = cp.to_vec();
    control[n - 1] = control[0];
    NurbsCurve::new(
        c.degree(),
        c.knots().knots().to_vec(),
        control,
        c.weights().to_vec(),
    )
    .map_err(|e| KernelError::InvalidCurve(e.to_string()))
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
        Segment::Nurbs { curve } => xy(curve.point(curve.domain().1)),
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

    /// Schleife aus einer einzigen geschlossenen Freiformkurve (etwa einer Vollellipse) in der
    /// xy-Ebene. Fehler `InvalidCurve`, wenn die Kurve nicht in der Ebene `z = 0` liegt,
    /// `ProfileNotClosed`, wenn Anfang und Ende weiter als die Längentoleranz auseinanderliegen.
    pub fn nurbs_loop(curve: NurbsCurve) -> Result<ProfileLoop, KernelError> {
        let tol = Tolerance::default();
        check_nurbs_plane(&curve, &tol)?;
        let (a, b) = curve.domain();
        if xy(curve.point(a)).distance(xy(curve.point(b))) > tol.linear {
            return Err(KernelError::ProfileNotClosed);
        }
        // Anfang und Ende exakt zusammenlegen (die geschlossene Wandfläche verlangt gleiche
        // Randreihen, viel enger als `tol.linear`).
        let curve = snap_closed(&curve, &tol)?;
        Ok(ProfileLoop {
            start: xy(curve.point(a)),
            segments: vec![Segment::Nurbs {
                curve: Arc::new(curve),
            }],
        })
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

    /// Punktfolge entlang der Kurve; Bögen werden mit `steps_per_turn` Schritten pro Vollkreis genähert,
    /// Freiformsegmente mit den Sehnenenden aus [`nurbs_chords`] (je Bézier-Stück mindestens eine
    /// Sehne bis zu `MAX_NURBS_CHORDS`; dieselben Punkte wie [`SampledLoop::new`]).
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
                Segment::Nurbs { curve } => {
                    out.extend(nurbs_chords(curve, p, steps_per_turn, MAX_NURBS_CHORDS).0);
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

    /// Dieselbe Kurve in umgekehrter Richtung; Fehler `InvalidCurve`, wenn sich ein
    /// Freiformsegment nicht umkehren lässt.
    pub fn try_reversed(&self) -> Result<ProfileLoop, KernelError> {
        let pts = self.chain_points();
        let m = self.segments.len();
        let segments = (0..m)
            .rev()
            .map(|i| {
                Ok(match &self.segments[i] {
                    Segment::Line { .. } => Segment::Line { to: pts[i] },
                    Segment::Arc { center, sweep } => Segment::Arc {
                        center: *center,
                        sweep: -sweep,
                    },
                    Segment::Nurbs { curve } => Segment::Nurbs {
                        curve: Arc::new(reverse_curve(curve)?),
                    },
                })
            })
            .collect::<Result<Vec<_>, KernelError>>()?;
        Ok(ProfileLoop {
            start: pts[m],
            segments,
        })
    }
}

/// Ebenes Profil: eine äußere Schleife und beliebig viele Löcher.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub outer: ProfileLoop,
    pub holes: Vec<ProfileLoop>,
}

impl Profile {
    /// Äußere Schleife gegen, Löcher im Uhrzeigersinn; Fehler `InvalidCurve`, wenn sich ein
    /// Freiformsegment nicht umkehren lässt (es gibt keine Fassung ohne Fehlerrückgabe mehr).
    pub fn try_oriented(&self) -> Result<Profile, KernelError> {
        let outer = if self.outer.signed_area() < 0.0 {
            self.outer.try_reversed()?
        } else {
            self.outer.clone()
        };
        let holes = self
            .holes
            .iter()
            .map(|h| {
                if h.signed_area() > 0.0 {
                    h.try_reversed()
                } else {
                    Ok(h.clone())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Profile { outer, holes })
    }
}

/// Liegt die Freiformkurve in der Ebene `z = 0`? Geprüft an den Kontrollpunkten (die Kurve liegt
/// in deren konvexer Hülle, die Gewichte sind positiv), Abweichung höchstens `tol.linear`.
fn check_nurbs_plane(curve: &NurbsCurve, tol: &Tolerance) -> Result<(), KernelError> {
    if curve
        .control_points()
        .iter()
        .any(|p| !p.is_finite() || p.z.abs() > tol.linear)
    {
        return Err(KernelError::InvalidCurve(
            "die Kurve liegt nicht in der Profilebene".into(),
        ));
    }
    Ok(())
}

/// Prüft ein Freiformsegment, das bei `from` beginnt: in der Profilebene, Anfang am Ende des
/// vorigen Segments (innerhalb `tol.linear`), nicht zu einem Punkt zusammengefallen; eine in sich
/// geschlossene Kurve nur als einziges Segment der Schleife (`alone`).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_nurbs_segment(
    curve: &NurbsCurve,
    from: DVec2,
    alone: bool,
    tol: &Tolerance,
) -> Result<(), KernelError> {
    check_nurbs_plane(curve, tol)?;
    let (a, b) = curve.domain();
    if !(xy(curve.point(a)).distance(from) <= tol.linear) {
        return Err(KernelError::InvalidCurve(
            "die Kurve schließt nicht an das vorige Segment an".into(),
        ));
    }
    let cp = curve.control_points();
    let spread = cp.iter().map(|p| p.distance(cp[0])).fold(0.0, f64::max);
    // Die negierte Form lehnt auch NaN ab.
    if !(spread > tol.linear) {
        return Err(KernelError::DegenerateSegment);
    }
    if !alone && xy(curve.point(b)).distance(from) <= tol.linear {
        return Err(KernelError::DegenerateSegment);
    }
    Ok(())
}

/// Prüft eine Schleife, bevor sie abgetastet wird: nicht leer, alle Werte endlich,
/// jeder Bogen mit Öffnungswinkel zwischen `tol.angular` und einer Vollkreisdrehung,
/// jedes Freiformsegment wie in [`check_nurbs_segment`].
pub(crate) fn check_loop_input(lp: &ProfileLoop, tol: &Tolerance) -> Result<(), KernelError> {
    if lp.segments.is_empty() || !lp.start.is_finite() {
        return Err(KernelError::DegenerateSegment);
    }
    let alone = lp.segments.len() == 1;
    let mut from = lp.start;
    for seg in &lp.segments {
        let ok = match seg {
            Segment::Line { to } => to.is_finite(),
            Segment::Arc { center, sweep } => {
                center.is_finite()
                    && sweep.is_finite()
                    && sweep.abs() > tol.angular
                    && sweep.abs() <= TAU + 1e-9
            }
            Segment::Nurbs { curve } => {
                check_nurbs_segment(curve, from, alone, tol)?;
                true
            }
        };
        if !ok {
            return Err(KernelError::DegenerateSegment);
        }
        from = segment_end(from, seg);
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
    let cap = profile_nurbs_cap(profile);
    let outer = SampledLoop::with_nurbs_cap(&profile.outer, LAYOUT_STEPS_PER_TURN, tol, cap);
    let holes: Vec<SampledLoop> = profile
        .holes
        .iter()
        .map(|h| SampledLoop::with_nurbs_cap(h, LAYOUT_STEPS_PER_TURN, tol, cap))
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

/// Sehnen je Freiformsegment in den Prüfungen eines Profils: `PROFILE_NURBS_CHORD_BUDGET` verteilt
/// auf alle Freiformsegmente, höchstens `MAX_NURBS_CHORDS`.
fn profile_nurbs_cap(profile: &Profile) -> usize {
    let count = std::iter::once(&profile.outer)
        .chain(profile.holes.iter())
        .flat_map(|lp| lp.segments.iter())
        .filter(|s| matches!(s, Segment::Nurbs { .. }))
        .count();
    (PROFILE_NURBS_CHORD_BUDGET / count.max(1)).clamp(1, MAX_NURBS_CHORDS)
}

/// Prüft jede Schleife mit Freiformsegmenten auf Selbstschnitt und Selbstberührung
/// (`ProfileSelfIntersects`). Schleifen nur aus Geraden und Bögen bleiben wie bisher ungeprüft
/// (die Skizze prüft sie selbst, das Verhalten des Kernels für sie ändert sich nicht). Eine
/// offene Kette (Rotation) gilt dabei als geradlinig (über die Achse) geschlossen.
pub(crate) fn check_self_intersection(
    profile: &Profile,
    tol: &Tolerance,
) -> Result<(), KernelError> {
    let cap = profile_nurbs_cap(profile);
    for lp in std::iter::once(&profile.outer).chain(profile.holes.iter()) {
        if !lp
            .segments
            .iter()
            .any(|s| matches!(s, Segment::Nurbs { .. }))
        {
            continue;
        }
        let sampled = SampledLoop::with_nurbs_cap(lp, LAYOUT_STEPS_PER_TURN, tol, cap);
        if sampled.touches_itself(tol.linear) {
            return Err(KernelError::ProfileSelfIntersects);
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
        Self::with_nurbs_cap(lp, steps_per_turn, tol, MAX_NURBS_CHORDS)
    }

    /// Wie [`SampledLoop::new`], mit höchstens `cap` Sehnen je Freiformsegment (mindestens 1).
    /// Die Pfeilhöhen bleiben sichere Schranken, die Prüfungen werden mit weniger Sehnen nur
    /// vorsichtiger.
    pub fn with_nurbs_cap(
        lp: &ProfileLoop,
        steps_per_turn: usize,
        tol: &Tolerance,
        cap: usize,
    ) -> SampledLoop {
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
                Segment::Nurbs { curve } => {
                    // Sehnen mit sicherer Schranke aus der konvexen Hülle (siehe `nurbs_chords`).
                    let (q, sag) = nurbs_chords(curve, p, steps_per_turn, cap);
                    points.extend(q);
                    into.extend(sag);
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

    /// Hüllrechteck der Sehne `i`, um `margin` plus ihre Pfeilhöhe vergrößert.
    fn chord_box(&self, i: usize, margin: f64) -> (DVec2, DVec2) {
        let (p, q) = self.segment(i);
        let m = DVec2::splat(margin + self.sagitta[i]);
        (p.min(q) - m, p.max(q) + m)
    }

    /// Schneiden oder berühren sich die beiden Schleifen? Zwei Sehnen gelten als berührend, wenn
    /// ihr Abstand höchstens `linear` plus ihre beiden Pfeilhöhen beträgt. Vorauswahl der Paare
    /// über ein Gitter (siehe [`close_pairs`]), damit lange Abtastungen nicht quadratisch kosten.
    pub fn touches(&self, other: &SampledLoop, linear: f64) -> bool {
        if !self.boxes_near(other, linear) {
            return false;
        }
        let a: Vec<_> = (0..self.points.len())
            .map(|i| self.chord_box(i, linear))
            .collect();
        let b: Vec<_> = (0..other.points.len())
            .map(|j| other.chord_box(j, 0.0))
            .collect();
        close_pairs(&a, &b, |i, j| {
            let (p, q) = self.segment(i);
            let (r, s) = other.segment(j);
            segment_distance(p, q, r, s) <= linear + self.sagitta[i] + other.sagitta[j]
        })
    }

    /// Kommen sich zwei nicht benachbarte Sehnen derselben Schleife näher als `linear` plus
    /// ihre Pfeilhöhen (Selbstschnitt oder Selbstberührung)? Vorauswahl wie bei [`Self::touches`].
    pub fn touches_itself(&self, linear: f64) -> bool {
        let n = self.points.len();
        if n < 4 {
            // Mit höchstens drei Sehnen ist jedes Paar benachbart.
            return false;
        }
        let a: Vec<_> = (0..n).map(|i| self.chord_box(i, linear)).collect();
        let b: Vec<_> = (0..n).map(|j| self.chord_box(j, 0.0)).collect();
        close_pairs(&a, &b, |i, j| {
            let (i, j) = (i.min(j), i.max(j));
            if j < i + 2 || (i == 0 && j == n - 1) {
                return false; // dieselbe oder benachbarte Sehne
            }
            let (p, q) = self.segment(i);
            let (r, s) = self.segment(j);
            segment_distance(p, q, r, s) <= linear + self.sagitta[i] + self.sagitta[j]
        })
    }

    /// Liegt `p` im Inneren des Linienzugs (Strahlverfahren, gerade/ungerade Schnittzahl)?
    /// Eindeutig für Punkte, die weiter als der Spielraum vom Rand entfernt sind.
    pub fn contains(&self, p: DVec2) -> bool {
        point_in_polygon(p, &self.points)
    }
}

/// Höchstzahl der Gitterzellen je Richtung in [`close_pairs`].
const MAX_GRID_CELLS_PER_AXIS: usize = 1024;

/// Gibt es ein Paar `(i, j)` sich überlappender Rechtecke `a[i]`, `b[j]`, für das `test` gilt?
/// Breitenphase über ein gleichmäßiges Gitter über den Rechtecken von `b` (etwa √|b| Zellen je
/// Richtung): jedes Paar mit überlappenden Rechtecken wird genau einmal geprüft, Paare ohne
/// Überlappung nie. Gibt es nicht endliche Rechtecke, wird jedes Paar geprüft.
fn close_pairs(
    a: &[(DVec2, DVec2)],
    b: &[(DVec2, DVec2)],
    mut test: impl FnMut(usize, usize) -> bool,
) -> bool {
    let overlap = |x: &(DVec2, DVec2), y: &(DVec2, DVec2)| {
        x.0.x <= y.1.x && y.0.x <= x.1.x && x.0.y <= y.1.y && y.0.y <= x.1.y
    };
    let finite = |r: &(DVec2, DVec2)| r.0.is_finite() && r.1.is_finite();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if !a.iter().all(finite) || !b.iter().all(finite) {
        // Ohne endliche Lage keine Vorauswahl (nur bei unendlichen Pfeilhöhen).
        return (0..a.len()).any(|i| (0..b.len()).any(|j| test(i, j)));
    }
    let (lo, hi) = b.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), r| (lo.min(r.0), hi.max(r.1)),
    );
    let g = ((b.len() as f64).sqrt().ceil() as usize).clamp(1, MAX_GRID_CELLS_PER_AXIS);
    let size = (hi - lo) / g as f64;
    let cell = |x: f64, lo: f64, size: f64| -> usize {
        if size > 0.0 {
            (((x - lo) / size).floor().max(0.0) as usize).min(g - 1)
        } else {
            0
        }
    };
    let range = |r: &(DVec2, DVec2)| {
        (
            cell(r.0.x, lo.x, size.x),
            cell(r.1.x, lo.x, size.x),
            cell(r.0.y, lo.y, size.y),
            cell(r.1.y, lo.y, size.y),
        )
    };
    let mut cells: Vec<Vec<u32>> = vec![Vec::new(); g * g];
    for (j, r) in b.iter().enumerate() {
        let (x0, x1, y0, y1) = range(r);
        for y in y0..=y1 {
            for x in x0..=x1 {
                cells[y * g + x].push(j as u32);
            }
        }
    }
    let bounds = (lo, hi);
    // Zuletzt geprüftes `i` je `j` (plus 1), damit kein Paar doppelt geprüft wird.
    let mut seen = vec![0usize; b.len()];
    for (i, r) in a.iter().enumerate() {
        if !overlap(r, &bounds) {
            continue;
        }
        let (x0, x1, y0, y1) = range(r);
        for y in y0..=y1 {
            for x in x0..=x1 {
                for &j in &cells[y * g + x] {
                    let j = j as usize;
                    if seen[j] == i + 1 {
                        continue;
                    }
                    seen[j] = i + 1;
                    if overlap(r, &b[j]) && test(i, j) {
                        return true;
                    }
                }
            }
        }
    }
    false
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
