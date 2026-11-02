//! Geometrie des Versatzes: reiner Plan, die Skizze wird nur gelesen. Vorschau und Klick rechnen
//! mit denselben Funktionen.
//!
//! **Rechnung** (`offset_plan`): Rohstücke (Linie verschoben, Bogen und Kreis `r ± d`, Ellipse,
//! Ellipsenbogen und Spline als Näherung durch exakte Versatzpunkte). An tangentialen Fugen ein
//! gemeinsamer Punkt, an scharfen der Schnitt der verlängerten Nachbarn, der der Vorlage am
//! nächsten liegt. Stücke, die dabei die Richtung umkehren, entfallen, das zuerst
//! zusammengelaufene zuerst; danach werden die Fugen neu gerechnet.
//!
//! **Trimmen:** Alle übrigen Stücke werden an allen Schnitten geteilt (`intersect_prepared`,
//! `split`, `Nodes`). Ein Teilstück bleibt, wenn seine Mitte auf der Versatzseite einer
//! geschlossenen Kette liegt, weder näher als `|d| − 10·tol` an einer Vorlagekurve noch in der
//! Gehrung einer anderen Ecke. Zwei Teilstücke auf
//! denselben Knoten und derselben Lage (kollineare Nachbarn nach einem verschwundenen Steg):
//! gleich gerichtet zählt eines, gegenläufig ist dort kein Rand mehr, und beide entfallen. Übrig
//! bleiben Knoten mit einem Zu- und einem Ablauf, sonst `SelfCrossing`. Offene Enden gibt es nur
//! an offenen Ketten; an den Enden offener Ketten schneidet ein Kreis mit `|d|` die Überstände.
//!
//! Treffen sich zwei Nachbarn nicht, bleiben ihre rohen Enden stehen: Liegen sie im Bereich, den
//! das Trimmen entfernt (Steg, Schenkel, die verschwinden), ist alles gut; sonst `NoJoin`. Zwei
//! Linien auf derselben Geraden laufen durch; ein kleiner werdender Bogen, den sein Nachbar nicht
//! mehr trifft, entfällt.

use crate::arrangement::{Nodes, polygon_contains, split};
use crate::curves::Sampled;
use crate::fillet::{Shape, meet, wrap_pi};
use crate::geom::Geom;
use crate::intersect::{Prepared, boxes_touch, intersect, intersect_prepared, self_intersect};
use crate::model::{CurveId, CurveKind, MAX_COORD, Sketch};
use crate::shapes::ArcPlan;
use planora_math::{DVec2, PI, TAU, Tolerance};
use std::collections::{HashMap, HashSet};

/// Abschnitte je Hälfte einer ganzen Ellipse.
const ELLIPSE_HALF_SECTIONS: usize = 32;
/// Abschnitte je Stützabschnitt eines Splines.
const SECTIONS_PER_SPAN: usize = 4;
/// Höchstzahl der Abschnitte eines Näherungsstücks.
const MAX_SECTIONS: usize = 63;
/// Abtastung der Krümmung beim Suchen nach Spitzen.
const CUSP_STEPS_PER_TURN: usize = 256;
const CUSP_STEPS_PER_SEGMENT: usize = 64;
/// Spielraum in Toleranzen beim Abstand zur Vorlage und beim Vergleich von Stückmitten.
const SLACK: f64 = 10.0;
/// Geschlossene Teilstücke bis zu so vielen Toleranzen Länge sind keine Stücke.
const TINY_LOOP: f64 = 4.0;
/// `1 − κ·d` ab hier gilt als Spitze.
const CUSP_MARGIN: f64 = 1e-9;

/// Gerichtete Kette. `bool`: gegen die Kurvenrichtung.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedChain {
    pub items: Vec<(CurveId, bool)>,
    pub closed: bool,
}

/// Geometrie eines Stücks. Linie und `Approx` laufen in Richtung der Schleife, `Arc` immer gegen
/// den Uhrzeigersinn (die Fugen zeigen, wie die Schleife ihn durchläuft).
#[derive(Clone, Debug, PartialEq)]
pub enum OffsetGeom {
    Line { a: DVec2, b: DVec2 },
    Arc(ArcPlan),
    Circle { center: DVec2, radius: f64 },
    Approx { points: Vec<DVec2> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct OffsetPiece {
    pub source: CurveId,
    pub geom: OffsetGeom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OffsetLoop {
    pub pieces: Vec<OffsetPiece>,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OffsetPlan {
    pub loops: Vec<OffsetLoop>,
    pub distance: f64,
    /// Vorlagekurven ohne Stück im Ergebnis.
    pub dropped: usize,
    /// Kurven der geschlossenen Vorlagen, für [`OffsetPlan::is_closed_like_source`].
    closed_sources: Vec<CurveId>,
}

impl OffsetPlan {
    /// Linienzug je Schleife in deren Richtung; bei geschlossenen ist der letzte Punkt der erste.
    pub fn outline(&self, steps_per_turn: usize) -> Vec<Vec<DVec2>> {
        self.loops
            .iter()
            .map(|l| loop_outline(l, steps_per_turn.max(1)))
            .collect()
    }

    pub fn piece_count(&self) -> usize {
        self.loops.iter().map(|l| l.pieces.len()).sum()
    }

    /// Schleifen aus geschlossenen Vorlagen sind geschlossen, solche aus offenen offen.
    pub fn is_closed_like_source(&self) -> bool {
        self.loops.iter().all(|l| {
            let from_closed = l
                .pieces
                .iter()
                .any(|p| self.closed_sources.contains(&p.source));
            l.closed == from_closed
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetProblem {
    /// Die Kette verzweigt sich.
    Branching,
    /// Vom Versatz bleibt nichts.
    Vanished,
    /// Zwei Nachbarn treffen sich nicht.
    NoJoin,
    /// Das Ergebnis kreuzt sich so, dass keine eindeutige Schleife bleibt.
    SelfCrossing,
    /// Der Abstand liegt über dem kleinsten Krümmungsradius einer Näherungskurve.
    Cusp,
    /// Unbrauchbare Eingabe: Abstand 0 oder nicht endlich, keine Kette, tote oder leere Kurve.
    Degenerate,
}

// ---------------------------------------------------------------------------------------------
// Linienzüge
// ---------------------------------------------------------------------------------------------

fn arc_geom(a: &ArcPlan) -> Geom {
    Geom::Arc {
        c: a.center,
        r: a.radius(),
        a0: (a.start - a.center).to_angle(),
        sweep: a.sweep(),
    }
}

fn spline_geom(points: &[DVec2]) -> Geom {
    Geom::Spline {
        s: Sampled::spline(points),
        n: points.len(),
    }
}

/// Mögliche Anfänge eines Stücks in Laufrichtung (nur beim Bogen zwei).
fn start_points(p: &OffsetPiece) -> Vec<DVec2> {
    match &p.geom {
        OffsetGeom::Line { a, .. } => vec![*a],
        OffsetGeom::Arc(arc) => vec![arc.start, arc.end],
        OffsetGeom::Circle { center, radius } => vec![*center + DVec2::new(*radius, 0.0)],
        OffsetGeom::Approx { points } => vec![points[0]],
    }
}

fn piece_polyline(p: &OffsetPiece, steps: usize) -> Vec<DVec2> {
    match &p.geom {
        OffsetGeom::Line { a, b } => vec![*a, *b],
        OffsetGeom::Arc(arc) => {
            let g = arc_geom(arc);
            let (lo, hi) = g.domain();
            let mut pl = g.polyline(lo, hi, steps);
            let last = pl.len() - 1;
            pl[0] = arc.start;
            pl[last] = arc.end;
            pl
        }
        OffsetGeom::Circle { center, radius } => {
            let g = Geom::Circle {
                c: *center,
                r: *radius,
            };
            let mut pl = g.polyline(0.0, TAU, steps);
            let last = pl.len() - 1;
            pl[last] = pl[0];
            pl
        }
        OffsetGeom::Approx { points } => {
            let g = spline_geom(points);
            let (lo, hi) = g.domain();
            let mut pl = g.polyline(lo, hi, steps);
            let last = pl.len() - 1;
            pl[0] = points[0];
            pl[last] = points[points.len() - 1];
            pl
        }
    }
}

fn loop_outline(l: &OffsetLoop, steps: usize) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::new();
    let n = l.pieces.len();
    for (i, p) in l.pieces.iter().enumerate() {
        let mut pl = piece_polyline(p, steps);
        if let OffsetGeom::Arc(arc) = &p.geom {
            let backwards = match out.last() {
                Some(prev) => arc.start != *prev && arc.end == *prev,
                None if n > 1 && (l.closed || i + 1 < n) => {
                    let next = start_points(&l.pieces[(i + 1) % n]);
                    !next.contains(&arc.end) && next.contains(&arc.start)
                }
                None => false,
            };
            if backwards {
                pl.reverse();
            }
        }
        let skip = usize::from(out.last() == pl.first());
        out.extend_from_slice(&pl[skip..]);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Rohstücke
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum RawKind {
    Line,
    /// Winkelbereich wie an der Vorlage.
    Round {
        c: DVec2,
        r: f64,
        a0: f64,
        sweep: f64,
        /// Der Radius wird kleiner als an der Vorlage.
        shrinks: bool,
    },
    Whole {
        c: DVec2,
        r: f64,
    },
    Approx {
        points: Vec<DVec2>,
    },
}

/// Versatz einer Vorlagekurve oder eines Stücks davon, vor dem Fügen.
#[derive(Clone, Debug)]
struct Raw {
    source: CurveId,
    kind: RawKind,
    /// Der Lauf geht gegen die Richtung von `kind` (nur beim Bogen von Bedeutung).
    against: bool,
    /// Vorlagepunkte und Einheitstangenten in Laufrichtung.
    from: DVec2,
    to: DVec2,
    dir_from: DVec2,
    dir_to: DVec2,
    raw_from: DVec2,
    raw_to: DVec2,
}

/// Enden einer offenen Kurve.
fn open_ends(sketch: &Sketch, c: CurveId) -> Option<[DVec2; 2]> {
    let (a, b) = match &sketch.curve(c)?.kind {
        CurveKind::Line { a, b } => (*a, *b),
        CurveKind::Arc { start, end, .. } | CurveKind::EllipticArc { start, end, .. } => {
            (*start, *end)
        }
        CurveKind::Spline { points } => (*points.first()?, *points.last()?),
        CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => return None,
    };
    Some([sketch.pos(a), sketch.pos(b)])
}

/// Einheitstangente in Laufrichtung.
fn unit_tangent(g: &Geom, t: f64, against: bool) -> Option<DVec2> {
    let d = g.eval(t).1.try_normalize()?;
    Some(if against { -d } else { d })
}

/// Versatzpunkte einer Näherung in Laufrichtung: Vorlagepunkt, Tangente, Versatzpunkt.
fn approx_samples(
    g: &Geom,
    ts: &[f64],
    against: bool,
    d: f64,
) -> Option<Vec<(DVec2, DVec2, DVec2)>> {
    let mut out = Vec::with_capacity(ts.len());
    for &t in ts {
        let dir = unit_tangent(g, t, against)?;
        let at = g.eval(t).0;
        out.push((at, dir, at + dir.perp() * d));
    }
    if against {
        out.reverse();
    }
    Some(out)
}

/// `Err(Cusp)`, wenn der Versatz `d` nach links die Kurve irgendwo umklappen ließe.
fn check_cusp(g: &Geom, against: bool, d: f64) -> Result<(), OffsetProblem> {
    let (lo, hi) = g.domain();
    let steps = match g {
        Geom::Spline { n, .. } => (n - 1) * CUSP_STEPS_PER_SEGMENT,
        _ => (((hi - lo) / TAU * CUSP_STEPS_PER_TURN as f64).ceil() as usize).max(1),
    };
    let sign = if against { -1.0 } else { 1.0 };
    for k in 0..=steps {
        let t = lo + (hi - lo) * k as f64 / steps as f64;
        if 1.0 - sign * g.curvature(t) * d <= CUSP_MARGIN {
            return Err(OffsetProblem::Cusp);
        }
    }
    Ok(())
}

impl Sketch {
    /// Rohstücke einer Kurve (eins, bei der Ellipse zwei, keins bei zu kleinem Radius).
    fn raw_pieces(
        &self,
        c: CurveId,
        against: bool,
        g: &Geom,
        d: f64,
        tol: f64,
    ) -> Result<Vec<Raw>, OffsetProblem> {
        const BAD: OffsetProblem = OffsetProblem::Degenerate;
        let (lo, hi) = g.domain();
        let shrink = |r: f64| if against { r + d } else { r - d };
        let finish = |kind: RawKind, points: Option<&Vec<(DVec2, DVec2, DVec2)>>| {
            let ends = open_ends(self, c).ok_or(BAD)?;
            let (from, to) = if against {
                (ends[1], ends[0])
            } else {
                (ends[0], ends[1])
            };
            let (t_from, t_to) = if against { (hi, lo) } else { (lo, hi) };
            let dir_from = unit_tangent(g, t_from, against).ok_or(BAD)?;
            let dir_to = unit_tangent(g, t_to, against).ok_or(BAD)?;
            let (raw_from, raw_to) = match points {
                Some(s) => (s[0].2, s[s.len() - 1].2),
                None => (from + dir_from.perp() * d, to + dir_to.perp() * d),
            };
            Ok(Raw {
                source: c,
                kind,
                against,
                from,
                to,
                dir_from,
                dir_to,
                raw_from,
                raw_to,
            })
        };
        match g {
            Geom::Line { .. } => Ok(vec![finish(RawKind::Line, None)?]),
            Geom::Arc { c: m, r, a0, sweep } => {
                let rr = shrink(*r);
                if rr <= tol {
                    return Ok(Vec::new());
                }
                let kind = RawKind::Round {
                    c: *m,
                    r: rr,
                    a0: *a0,
                    sweep: *sweep,
                    shrinks: rr < *r,
                };
                Ok(vec![finish(kind, None)?])
            }
            Geom::Circle { c: m, r } => {
                let rr = shrink(*r);
                if rr <= tol {
                    return Ok(Vec::new());
                }
                // Für die Fugen ohne Bedeutung: Ein Kreis ist immer allein.
                let p = *m + DVec2::new(rr, 0.0);
                Ok(vec![Raw {
                    source: c,
                    kind: RawKind::Whole { c: *m, r: rr },
                    against,
                    from: p,
                    to: p,
                    dir_from: DVec2::Y,
                    dir_to: DVec2::Y,
                    raw_from: p,
                    raw_to: p,
                }])
            }
            Geom::Ellipse { .. } => {
                check_cusp(g, against, d)?;
                let sections = 2 * ELLIPSE_HALF_SECTIONS;
                let ts: Vec<f64> = (0..sections)
                    .map(|k| TAU * k as f64 / sections as f64)
                    .collect();
                let mut all = approx_samples(g, &ts, against, d).ok_or(BAD)?;
                all.push(all[0]);
                Ok([
                    &all[..=ELLIPSE_HALF_SECTIONS],
                    &all[ELLIPSE_HALF_SECTIONS..],
                ]
                .into_iter()
                .map(|half| approx_raw(c, half))
                .collect())
            }
            Geom::EllipticArc { sweep, .. } => {
                check_cusp(g, against, d)?;
                let sections = (sweep / PI * ELLIPSE_HALF_SECTIONS as f64).ceil() as usize;
                self.open_approx(c, g, sections.clamp(1, MAX_SECTIONS), against, d)
            }
            Geom::Spline { n, .. } => {
                check_cusp(g, against, d)?;
                let sections = ((n - 1) * SECTIONS_PER_SPAN).min(MAX_SECTIONS);
                self.open_approx(c, g, sections, against, d)
            }
        }
    }

    /// Näherung einer offenen Kurve, die Enden genau auf den Normalen der Skizzenpunkte.
    fn open_approx(
        &self,
        c: CurveId,
        g: &Geom,
        sections: usize,
        against: bool,
        d: f64,
    ) -> Result<Vec<Raw>, OffsetProblem> {
        let (lo, hi) = g.domain();
        let ts: Vec<f64> = (0..=sections)
            .map(|k| lo + (hi - lo) * k as f64 / sections as f64)
            .collect();
        let samples = approx_samples(g, &ts, against, d).ok_or(OffsetProblem::Degenerate)?;
        Ok(vec![approx_raw(c, &samples)])
    }
}

fn approx_raw(source: CurveId, samples: &[(DVec2, DVec2, DVec2)]) -> Raw {
    let (first, last) = (samples[0], samples[samples.len() - 1]);
    Raw {
        source,
        kind: RawKind::Approx {
            points: samples.iter().map(|s| s.2).collect(),
        },
        against: false,
        from: first.0,
        to: last.0,
        dir_from: first.1,
        dir_to: last.1,
        raw_from: first.2,
        raw_to: last.2,
    }
}

// ---------------------------------------------------------------------------------------------
// Fugen
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Join {
    /// Tangentiale Fuge.
    Shared(DVec2),
    /// Schnitt der verlängerten Nachbarn.
    Miter(DVec2),
    /// Die Nachbarn treffen sich nicht, ihre rohen Enden bleiben.
    Gap,
}

impl Join {
    fn at(self) -> Option<DVec2> {
        match self {
            Join::Shared(p) | Join::Miter(p) => Some(p),
            Join::Gap => None,
        }
    }
}

impl Raw {
    /// Träger am Ende des Laufs (`end`) oder an seinem Anfang.
    fn carrier(&self, end: bool) -> Option<Shape> {
        match &self.kind {
            RawKind::Line => Some(Shape::Line {
                a: self.raw_from,
                d: self.dir_from,
            }),
            RawKind::Round { c, r, .. } | RawKind::Whole { c, r } => {
                Some(Shape::Round { c: *c, r: *r })
            }
            RawKind::Approx { .. } => Some(if end {
                Shape::Line {
                    a: self.raw_to,
                    d: self.dir_to,
                }
            } else {
                Shape::Line {
                    a: self.raw_from,
                    d: self.dir_from,
                }
            }),
        }
    }

    /// Weit verlängert als Kurve, zum Schneiden mit einer Näherung.
    fn extended(&self, d: f64) -> Geom {
        match &self.kind {
            RawKind::Line => {
                let reach = 2.0 * (self.raw_from.distance(self.raw_to) + d.abs());
                Geom::Line {
                    a: self.raw_from - self.dir_from * reach,
                    b: self.raw_to + self.dir_from * reach,
                }
            }
            RawKind::Round { c, r, .. } | RawKind::Whole { c, r } => Geom::Circle { c: *c, r: *r },
            RawKind::Approx { points } => spline_geom(points),
        }
    }
}

fn join_of(p: &Raw, q: &Raw, adjacent: bool, d: f64, tol: &Tolerance) -> Join {
    let same_way = p.dir_to.dot(q.dir_from) > 0.0;
    if adjacent
        && same_way
        && (p.dir_to.perp_dot(q.dir_from).abs() <= tol.angular
            || p.raw_to.distance(q.raw_from) <= tol.linear)
    {
        return Join::Shared(p.to + p.dir_to.perp() * d);
    }
    if !adjacent
        && same_way
        && matches!((&p.kind, &q.kind), (RawKind::Line, RawKind::Line))
        && p.dir_to.perp_dot(q.dir_from).abs() <= tol.angular
        && p.dir_to.perp_dot(q.raw_from - p.raw_to).abs() <= tol.linear
    {
        // Dieselbe Gerade: Was dazwischen lag, ist verschwunden, die Kante läuft durch.
        return Join::Miter((p.raw_to + q.raw_from) * 0.5);
    }
    let reference = (p.to + q.from) * 0.5;
    let mut candidates = match (p.carrier(true), q.carrier(false)) {
        (Some(a), Some(b)) => meet(a, b),
        _ => Vec::new(),
    };
    if matches!(p.kind, RawKind::Approx { .. }) || matches!(q.kind, RawKind::Approx { .. }) {
        let (gp, gq) = (p.extended(d), q.extended(d));
        candidates.extend(
            intersect(&gp, &gq, tol.linear)
                .into_iter()
                .map(|(t, _)| gp.eval(t).0),
        );
    }
    candidates
        .into_iter()
        .filter(|x| x.is_finite() && x.abs().max_element() <= MAX_COORD)
        .min_by(|a, b| a.distance(reference).total_cmp(&b.distance(reference)))
        .map_or(Join::Gap, Join::Miter)
}

/// Rohstück mit gefügten Enden.
struct Cut {
    geom: Geom,
    points: Option<Vec<DVec2>>,
    /// Länge in Laufrichtung, kleiner oder gleich 0: umgekehrt.
    length: f64,
    /// Länge ohne Fugen.
    full: f64,
}

fn cut(raw: &Raw, js: DVec2, je: DVec2, tol: f64) -> Cut {
    match &raw.kind {
        RawKind::Line => Cut {
            geom: Geom::Line { a: js, b: je },
            points: None,
            length: (je - js).dot(raw.dir_from),
            full: raw.from.distance(raw.to),
        },
        RawKind::Round {
            c, r, a0, sweep, ..
        } => {
            let (start, end) = if raw.against { (je, js) } else { (js, je) };
            let a_end = a0 + sweep;
            let t0 = a0 + wrap_pi((start - *c).to_angle() - a0);
            let t1 = a_end + wrap_pi((end - *c).to_angle() - a_end);
            let span = t1 - t0;
            Cut {
                geom: Geom::Arc {
                    c: *c,
                    r: *r,
                    a0: t0,
                    sweep: span,
                },
                points: None,
                length: span * r,
                full: sweep * r,
            }
        }
        RawKind::Whole { c, r } => Cut {
            geom: Geom::Circle { c: *c, r: *r },
            points: None,
            length: TAU * r,
            full: TAU * r,
        },
        RawKind::Approx { points } => cut_approx(points, js, je, tol),
    }
}

/// Näherung bis `js`/`je`: bleibt bitgleich, wo die Fuge schon ihr Ende ist; liegt die Fuge auf
/// ihr, wird gekürzt; sonst liegt sie auf der Verlängerung und kommt dazu.
fn cut_approx(points: &[DVec2], js: DVec2, je: DVec2, tol: f64) -> Cut {
    let g = spline_geom(points);
    let last = points.len() - 1;
    let place = |j: DVec2, end_value: usize| -> f64 {
        if j == points[end_value] {
            return end_value as f64;
        }
        let t = g.nearest(j);
        if g.eval(t).0.distance(j) <= tol {
            t
        } else {
            end_value as f64
        }
    };
    let (ts, te) = (place(js, 0), place(je, last));
    let mut kept = vec![js];
    kept.extend(
        (0..=last)
            .filter(|k| (*k as f64) > ts + 1e-9 && (*k as f64) < te - 1e-9)
            .map(|k| points[k]),
    );
    kept.push(je);
    let full = points.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
    let length = (te - ts) * full / last as f64;
    let n_ok = te > ts && kept.len() >= 2 && js != je;
    Cut {
        geom: if n_ok {
            spline_geom(&kept)
        } else {
            Geom::Line { a: js, b: je }
        },
        points: Some(kept),
        length: if n_ok { length.max(tol * 2.0) } else { 0.0 },
        full,
    }
}

/// Gehrung einer Ecke: Bereich zwischen Vorlage, rohen Enden und Schnittpunkt.
struct Kite {
    points: [DVec2; 5],
    bbox: (DVec2, DVec2),
}

impl Kite {
    fn new(points: [DVec2; 5]) -> Kite {
        let bbox = points
            .iter()
            .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
        Kite { points, bbox }
    }

    /// Echt innen, mit Abstand `margin` zum Rand.
    fn contains(&self, p: DVec2, margin: f64) -> bool {
        if !boxes_touch((p, p), self.bbox, 0.0) || !polygon_contains(&self.points, p) {
            return false;
        }
        (0..self.points.len()).all(|i| {
            let (a, b) = (self.points[i], self.points[(i + 1) % self.points.len()]);
            let d = b - a;
            let len2 = d.length_squared();
            let t = if len2 > 0.0 {
                ((p - a).dot(d) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.distance(a + d * t) > margin
        })
    }
}

/// Stück nach dem Fügen, vor dem Trimmen.
struct Settled {
    source: CurveId,
    chain: usize,
    against: bool,
    geom: Geom,
    points: Option<Vec<DVec2>>,
    start: DVec2,
    end: DVec2,
    gap_start: bool,
    gap_end: bool,
    /// Das nächste Stück (Index in der ganzen Liste), wenn die Fuge dorthin tangential ist.
    tangent_next: Option<usize>,
}

/// Drehung der Kette vom Ende von `raws[i]` bis zum Anfang von `raws[j]` samt der Stücke dazwischen.
fn turn_between(raws: &[Raw], i: usize, j: usize) -> f64 {
    let angle = |a: DVec2, b: DVec2| a.perp_dot(b).atan2(a.dot(b));
    let n = raws.len();
    let (mut sum, mut k) = (0.0, i);
    loop {
        let next = (k + 1) % n;
        sum += angle(raws[k].dir_to, raws[next].dir_from);
        if next == j {
            return sum;
        }
        sum += angle(raws[next].dir_from, raws[next].dir_to);
        k = next;
    }
}

/// Fuge über entfallene Stücke hinweg: Dreht die Kette dazwischen um mehr als 180°, laufen die
/// Träger auseinander, und ihr Schnitt liegt hinter ihrem Rücken. Das ist keine Fuge.
fn check_rejoin(join: Join, raws: &[Raw], i: usize, j: usize) -> Join {
    match join {
        Join::Miter(_) if turn_between(raws, i, j).abs() > PI => Join::Gap,
        _ => join,
    }
}

/// Fügt eine Kette; Stücke, die umkehren, entfallen, das zuerst zusammengelaufene zuerst.
fn settle_chain(
    raws: &[Raw],
    closed: bool,
    chain: usize,
    d: f64,
    tol: &Tolerance,
    out: &mut Vec<Settled>,
    kites: &mut Vec<Kite>,
) {
    let n = raws.len();
    let mut alive: Vec<usize> = (0..n).collect();
    loop {
        let m = alive.len();
        if m == 0 {
            return;
        }
        let single_whole = m == 1 && matches!(raws[alive[0]].kind, RawKind::Whole { .. });
        // `joins[k]`: Fuge von `alive[k]` zum Nachfolger.
        let joins: Vec<Option<Join>> = (0..m)
            .map(|k| {
                if single_whole || (!closed && k + 1 == m) {
                    return None;
                }
                let (i, j) = (alive[k], alive[(k + 1) % m]);
                let adjacent = (i + 1) % n == j && raws[i].to.distance(raws[j].from) <= tol.linear;
                let join = join_of(&raws[i], &raws[j], adjacent, d, tol);
                Some(if adjacent {
                    join
                } else {
                    check_rejoin(join, raws, i, j)
                })
            })
            .collect();
        let before = |k: usize| joins[(k + m - 1) % m];
        let cuts: Vec<Cut> = (0..m)
            .map(|k| {
                let raw = &raws[alive[k]];
                let js = before(k).and_then(Join::at).unwrap_or(raw.raw_from);
                let je = joins[k].and_then(Join::at).unwrap_or(raw.raw_to);
                cut(raw, js, je, tol.linear)
            })
            .collect();
        // Zusammenlauf: linear im Abstand, also lässt sich die Stelle ablesen.
        let mut collapse: Vec<Option<f64>> = cuts
            .iter()
            .map(|c| {
                (c.length <= tol.linear).then(|| {
                    let lost = c.full - c.length;
                    if lost > 0.0 {
                        d.abs() * c.full / lost
                    } else {
                        d.abs()
                    }
                })
            })
            .collect();
        // Ein kleiner werdender Bogen, den der Nachbar nicht mehr trifft, ist vorher verschwunden.
        for (k, join) in joins.iter().enumerate() {
            if *join != Some(Join::Gap) {
                continue;
            }
            let next = (k + 1) % m;
            let shrinking = |i: usize| match raws[alive[i]].kind {
                RawKind::Round {
                    r, shrinks: true, ..
                } => Some(r),
                _ => None,
            };
            let victim = match (shrinking(k), shrinking(next)) {
                (Some(a), Some(b)) => Some(if a <= b { k } else { next }),
                (Some(_), None) => Some(k),
                (None, Some(_)) => Some(next),
                (None, None) => None,
            };
            if let Some(v) = victim {
                collapse[v] = Some(collapse[v].unwrap_or(d.abs()));
            }
        }
        let first = collapse
            .iter()
            .flatten()
            .copied()
            .fold(f64::INFINITY, f64::min);
        if first.is_finite() {
            let limit = first * (1.0 + 1e-9) + 1e-12;
            let mut k = 0;
            alive.retain(|_| {
                let keep = !collapse[k].is_some_and(|c| c <= limit);
                k += 1;
                keep
            });
            continue;
        }
        let base = out.len();
        let m_raw = cuts.into_iter().enumerate();
        for (k, c) in m_raw {
            let raw = &raws[alive[k]];
            let before_join = before(k);
            let after_join = joins[k];
            let js = before_join.and_then(Join::at).unwrap_or(raw.raw_from);
            let je = after_join.and_then(Join::at).unwrap_or(raw.raw_to);
            let next = base + (k + 1) % m;
            out.push(Settled {
                source: raw.source,
                chain,
                // Linie und Näherung sind schon in Laufrichtung gebaut.
                against: raw.against
                    && matches!(raw.kind, RawKind::Round { .. } | RawKind::Whole { .. }),
                geom: c.geom,
                points: c.points,
                start: js,
                end: je,
                gap_start: before_join == Some(Join::Gap),
                gap_end: after_join == Some(Join::Gap),
                tangent_next: matches!(after_join, Some(Join::Shared(_))).then_some(next),
            });
            if let Some(Join::Miter(x)) = after_join {
                let q = &raws[alive[(k + 1) % m]];
                let (i, j) = (alive[k], alive[(k + 1) % m]);
                if d * turn_between(raws, i, j) < 0.0 {
                    kites.push(Kite::new([raw.to, raw.raw_to, x, q.raw_from, q.from]));
                }
            }
        }
        return;
    }
}

// ---------------------------------------------------------------------------------------------
// Trimmen
// ---------------------------------------------------------------------------------------------

/// Was geprüft und abgeschnitten wird.
struct Template {
    source: CurveId,
    geom: Geom,
    bbox: (DVec2, DVec2),
}

/// Teilstück in Parameterrichtung (`t0 < t1`); `from` und `to` in Laufrichtung.
#[derive(Clone, Copy)]
struct Sub {
    parent: usize,
    t0: f64,
    t1: f64,
    from: usize,
    to: usize,
    mid: DVec2,
}

/// Reichweite, in der ein Schnitt zweier tangentialer Nachbarn nur die Fuge selbst ist (zwei fast
/// gleiche Kurven liegen über `√(2·r·tol)` innerhalb von `tol`).
fn touching_reach(a: &Geom, b: &Geom, tol: f64) -> f64 {
    let radius = |g: &Geom| match g {
        Geom::Circle { r, .. } | Geom::Arc { r, .. } => *r,
        Geom::Line { .. } => 0.0,
        _ => 100.0,
    };
    4.0 * (2.0 * radius(a).max(radius(b)).max(1.0) * tol).sqrt()
}

struct TrimInput<'a> {
    pieces: &'a [Settled],
    templates: &'a [Template],
    kites: &'a [Kite],
    /// Enden offener Vorlagen.
    caps: &'a [DVec2],
    chains: &'a [OrderedChain],
    /// Geometrie je Kette, wie in `chains`.
    chain_geoms: &'a [Vec<Geom>],
    d: f64,
    tol: f64,
}

fn trim(input: &TrimInput) -> Result<Vec<OffsetLoop>, OffsetProblem> {
    let (pieces, tol) = (input.pieces, input.tol);
    let reach_d = input.d.abs();
    let mut nodes = Nodes::new(tol);
    let n = pieces.len();
    let mut events: Vec<Vec<(f64, usize)>> = vec![Vec::new(); n];
    // Enden zuerst: Ihre Lage vertritt den Knoten, darum sind die Fugen bitgleich.
    let mut end_nodes = vec![(0, 0); n];
    for (i, p) in pieces.iter().enumerate() {
        if !p.geom.periodic() {
            let (lo, hi) = p.geom.domain();
            let (s, e) = (nodes.get(p.start), nodes.get(p.end));
            end_nodes[i] = (s, e);
            let (at_lo, at_hi) = if p.against { (e, s) } else { (s, e) };
            events[i].push((lo, at_lo));
            events[i].push((hi, at_hi));
        }
    }
    let tangent: HashSet<(usize, usize)> = pieces
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.tangent_next.map(|j| (i.min(j), i.max(j))))
        .collect();
    let prepared: Vec<Prepared> = pieces.iter().map(|p| Prepared::new(&p.geom)).collect();
    for i in 0..n {
        for j in i + 1..n {
            let hits = intersect_prepared(&prepared[i], &prepared[j], tol);
            let reach = touching_reach(&pieces[i].geom, &pieces[j].geom, tol);
            let joint = tangent.contains(&(i, j)).then(|| {
                if pieces[i].tangent_next == Some(j) {
                    pieces[i].end
                } else {
                    pieces[j].end
                }
            });
            for (ti, tj) in hits {
                let p = (pieces[i].geom.eval(ti).0 + pieces[j].geom.eval(tj).0) * 0.5;
                if joint.is_some_and(|x| x.distance(p) <= reach) {
                    continue;
                }
                let v = nodes.get(p);
                events[i].push((ti, v));
                events[j].push((tj, v));
            }
        }
        let g = &pieces[i].geom;
        for (t1, t2) in self_intersect(g, tol) {
            let v = nodes.get((g.eval(t1).0 + g.eval(t2).0) * 0.5);
            events[i].push((t1, v));
            events[i].push((t2, v));
        }
    }
    // An den Enden offener Vorlagen hört der Bereich im Abstand `|d|` als Kreis auf.
    for cap in input.caps {
        let circle = Geom::Circle {
            c: *cap,
            r: reach_d,
        };
        let cp = Prepared::new(&circle);
        for (i, p) in prepared.iter().enumerate() {
            for (t, _) in intersect_prepared(p, &cp, tol) {
                events[i].push((t, nodes.get(pieces[i].geom.eval(t).0)));
            }
        }
    }
    for (i, p) in pieces.iter().enumerate() {
        if p.geom.periodic() && events[i].is_empty() {
            events[i].push((0.0, nodes.get(p.geom.eval(0.0).0)));
        }
    }

    // Teilstücke, die im richtigen Abstand liegen.
    let mut subs: Vec<Sub> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        for (t0, t1, a, b) in split(&p.geom, std::mem::take(&mut events[i]), tol) {
            if t1 <= t0 || (a == b && p.geom.arc_length(t0, t1) <= TINY_LOOP * tol) {
                continue;
            }
            let (from, to) = if p.against { (b, a) } else { (a, b) };
            let (check, own) = check_point(p, t0, t1);
            if valid_mid(input, check, own.then_some(p.source), p.chain) {
                subs.push(Sub {
                    parent: i,
                    t0,
                    t1,
                    from,
                    to,
                    mid: p.geom.eval(0.5 * (t0 + t1)).0,
                });
            }
        }
    }
    let subs = remove_doubles(subs, tol);
    if subs.is_empty() {
        return Err(OffsetProblem::Vanished);
    }

    // Jeder Knoten: ein Zulauf, ein Ablauf; offene Enden nur an offenen Ketten.
    let mut next_at: HashMap<usize, usize> = HashMap::new();
    let mut prev_at: HashMap<usize, usize> = HashMap::new();
    for (k, s) in subs.iter().enumerate() {
        if next_at.insert(s.from, k).is_some() || prev_at.insert(s.to, k).is_some() {
            return Err(OffsetProblem::SelfCrossing);
        }
    }
    let at_gap = |parent: usize, node: usize| {
        let p = &pieces[parent];
        (p.gap_start && end_nodes[parent].0 == node) || (p.gap_end && end_nodes[parent].1 == node)
    };
    for s in &subs {
        for (node, open) in [
            (s.from, !prev_at.contains_key(&s.from)),
            (s.to, !next_at.contains_key(&s.to)),
        ] {
            if open {
                if at_gap(s.parent, node) {
                    return Err(OffsetProblem::NoJoin);
                }
                if input.chains[pieces[s.parent].chain].closed {
                    return Err(OffsetProblem::SelfCrossing);
                }
            }
        }
    }

    // Schleifen ablaufen, sortiert nach dem ersten Teilstück.
    let mut seen = vec![false; subs.len()];
    let mut loops = Vec::new();
    for k in 0..subs.len() {
        if seen[k] {
            continue;
        }
        let mut start = k;
        let mut is_cycle = false;
        while let Some(&p) = prev_at.get(&subs[start].from) {
            start = p;
            if start == k {
                is_cycle = true;
                break;
            }
        }
        let mut order = Vec::new();
        let mut at = start;
        loop {
            seen[at] = true;
            order.push(at);
            match next_at.get(&subs[at].to) {
                Some(&q) if q != start => at = q,
                _ => break,
            }
        }
        // Teilstücke desselben Stücks gehören zusammen, auch über den Anfang der Schleife.
        if is_cycle {
            let first = subs[order[0]].parent;
            let tail = order
                .iter()
                .rev()
                .take_while(|&&q| subs[q].parent == first)
                .count();
            if tail < order.len() {
                order.rotate_right(tail);
            }
        }
        loops.push(build_loop(&subs, &order, pieces, &nodes.pos, is_cycle));
    }
    Ok(loops)
}

/// Punkt, an dem der Abstand eines Teilstücks geprüft wird, und ob die eigene Vorlage dort nicht
/// zählt: Näherungen weichen zwischen ihren Stützstellen von `|d|` ab, also zählt eine Stützstelle.
fn check_point(p: &Settled, t0: f64, t1: f64) -> (DVec2, bool) {
    let mid = 0.5 * (t0 + t1);
    let Some(points) = &p.points else {
        return (p.geom.eval(mid).0, false);
    };
    let k = mid.round();
    let inner = k > t0 + 1e-9 && k < t1 - 1e-9 && k > 0.0 && (k as usize) + 1 < points.len();
    if inner {
        (points[k as usize], false)
    } else {
        (p.geom.eval(mid).0, true)
    }
}

/// Liegt der Punkt auf der richtigen Seite, weder näher als `|d|` an der Vorlage noch in einer
/// Gehrung?
fn valid_mid(input: &TrimInput, p: DVec2, skip: Option<CurveId>, chain: usize) -> bool {
    let reach = input.d.abs();
    let limit = reach - SLACK * input.tol;
    // Über eine Gehrung kann ein Stück auf die andere Seite einer geschlossenen Kette geraten.
    if input.chains[chain].closed {
        let side = signed_distance(
            &input.chain_geoms[chain],
            &input.chains[chain],
            p,
            input.tol,
        );
        if side.is_some_and(|s| s * input.d < 0.0) {
            return false;
        }
    }
    let close = input.templates.iter().any(|t| {
        Some(t.source) != skip
            && boxes_touch((p, p), t.bbox, reach)
            && t.geom.eval(t.geom.nearest(p)).0.distance(p) < limit
    });
    !close && !input.kites.iter().any(|k| k.contains(p, SLACK * input.tol))
}

/// Teilstücke auf denselben Knoten und derselben Lage: gleich gerichtet eines, gegenläufig keines.
fn remove_doubles(subs: Vec<Sub>, tol: f64) -> Vec<Sub> {
    let mut by_nodes: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    let mut keep = vec![true; subs.len()];
    for (k, s) in subs.iter().enumerate() {
        let same = by_nodes
            .entry((s.from.min(s.to), s.from.max(s.to)))
            .or_default();
        let twin = same
            .iter()
            .copied()
            .find(|&o| keep[o] && subs[o].mid.distance(s.mid) < SLACK * tol);
        match twin {
            Some(o) => {
                keep[k] = false;
                if subs[o].from != s.from {
                    keep[o] = false;
                }
            }
            None => same.push(k),
        }
    }
    let mut k = 0;
    subs.into_iter()
        .filter(|_| {
            k += 1;
            keep[k - 1]
        })
        .collect()
}

fn build_loop(
    subs: &[Sub],
    order: &[usize],
    pieces: &[Settled],
    nodes: &[DVec2],
    closed: bool,
) -> OffsetLoop {
    let mut out: Vec<OffsetPiece> = Vec::new();
    let mut i = 0;
    while i < order.len() {
        let parent = subs[order[i]].parent;
        let mut j = i;
        while j + 1 < order.len() && subs[order[j + 1]].parent == parent {
            j += 1;
        }
        let run = &order[i..=j];
        let (from, to) = (subs[run[0]].from, subs[run[run.len() - 1]].to);
        let p = &pieces[parent];
        let (a, b) = (nodes[from], nodes[to]);
        let geom = match &p.geom {
            Geom::Line { .. } => OffsetGeom::Line { a, b },
            Geom::Circle { c, r } if from == to => OffsetGeom::Circle {
                center: *c,
                radius: *r,
            },
            Geom::Circle { c, .. } | Geom::Arc { c, .. } => {
                let (start, end) = if p.against { (b, a) } else { (a, b) };
                OffsetGeom::Arc(ArcPlan {
                    center: *c,
                    start,
                    end,
                })
            }
            _ => {
                let t0 = run
                    .iter()
                    .map(|k| subs[*k].t0)
                    .fold(f64::INFINITY, f64::min);
                let t1 = run
                    .iter()
                    .map(|k| subs[*k].t1)
                    .fold(f64::NEG_INFINITY, f64::max);
                let inner = p.points.as_deref().unwrap_or(&[]);
                let mut points = vec![a];
                points.extend(
                    (0..inner.len())
                        .filter(|k| (*k as f64) > t0 + 1e-9 && (*k as f64) < t1 - 1e-9)
                        .map(|k| inner[k]),
                );
                points.push(b);
                OffsetGeom::Approx { points }
            }
        };
        out.push(OffsetPiece {
            source: p.source,
            geom,
        });
        i = j + 1;
    }
    OffsetLoop {
        pieces: out,
        closed,
    }
}

/// Abstand zur Kette mit Vorzeichen, positiv links. Am Knick zählt die Summe der Tangenten beider
/// Seiten: Dort liegt der nächste Punkt nur für Punkte außerhalb des Knicks.
fn signed_distance(geoms: &[Geom], chain: &OrderedChain, p: DVec2, tol: f64) -> Option<f64> {
    let (k, t, dist) = geoms
        .iter()
        .enumerate()
        .map(|(k, g)| {
            let t = g.nearest(p);
            (k, t, g.eval(t).0.distance(p))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))?;
    if dist.is_nan() || dist <= tol {
        return None;
    }
    let n = geoms.len();
    let tangent_at = |i: usize, travel_start: bool| {
        let (lo, hi) = geoms[i].domain();
        let against = chain.items[i].1;
        let t = if travel_start != against { lo } else { hi };
        unit_tangent(&geoms[i], t, against)
    };
    let g = &geoms[k];
    let against = chain.items[k].1;
    let foot = g.eval(t).0;
    let mut dir = unit_tangent(g, t, against)?;
    if !g.periodic() {
        let (lo, hi) = g.domain();
        let (start_t, end_t) = if against { (hi, lo) } else { (lo, hi) };
        if t == end_t && (chain.closed || k + 1 < n) {
            dir += tangent_at((k + 1) % n, true)?;
        } else if t == start_t && (chain.closed || k > 0) {
            dir += tangent_at((k + n - 1) % n, false)?;
        }
    }
    let side = dir.perp_dot(p - foot);
    (side != 0.0).then(|| dist.copysign(side))
}

// ---------------------------------------------------------------------------------------------
// Skizze
// ---------------------------------------------------------------------------------------------

impl Sketch {
    /// Ordnet Kurven zu gerichteten Ketten. Enden gelten als verbunden, wenn sie in derselben
    /// Lage liegen. Verzweigung: `Branching`. Konstruktionskurven an einer normalen Kette zählen
    /// nicht. Eine geschlossene Kette beginnt an ihrer kleinsten Kurve in deren Richtung, eine
    /// offene an dem Ende mit der kleineren Kurve.
    pub fn ordered_chains(&self, curves: &[CurveId]) -> Result<Vec<OrderedChain>, OffsetProblem> {
        let tol = self.tolerance().linear;
        let mut ids = curves.to_vec();
        ids.sort();
        ids.dedup();
        let mut nodes = Nodes::new(tol);
        let mut links: Vec<Option<[usize; 2]>> = Vec::with_capacity(ids.len());
        for &c in &ids {
            let g = self.geom(c).ok_or(OffsetProblem::Degenerate)?;
            links.push(if g.periodic() {
                None
            } else {
                let [a, b] = open_ends(self, c).ok_or(OffsetProblem::Degenerate)?;
                Some([nodes.get(a), nodes.get(b)])
            });
        }
        let count = nodes.pos.len();
        let keep = self.without_construction_rim(&ids, &links, count);
        let mut at: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (i, l) in links.iter().enumerate() {
            if let (true, Some([a, b])) = (keep[i], l) {
                at[*a].push(i);
                at[*b].push(i);
            }
        }
        if at.iter().any(|list| list.len() > 2) {
            return Err(OffsetProblem::Branching);
        }
        let other = |cur: usize, node: usize| at[node].iter().copied().find(|&o| o != cur);
        let far = |cur: usize, node: usize| {
            let [a, b] = links[cur].expect("offene Kurve");
            if a == node { b } else { a }
        };
        let mut seen = vec![false; ids.len()];
        let mut chains = Vec::new();
        for i in 0..ids.len() {
            if seen[i] || !keep[i] {
                continue;
            }
            let Some([a, b]) = links[i] else {
                seen[i] = true;
                chains.push(OrderedChain {
                    items: vec![(ids[i], false)],
                    closed: true,
                });
                continue;
            };
            // Bis zum freien Ende laufen, in beide Richtungen.
            let run_to_end = |mut cur: usize, mut node: usize| -> Option<(usize, usize)> {
                loop {
                    match other(cur, node) {
                        None => return Some((cur, node)),
                        Some(o) if o == i => return None,
                        Some(o) => {
                            node = far(o, node);
                            cur = o;
                        }
                    }
                }
            };
            let self_loop = a == b;
            let back = if self_loop { None } else { run_to_end(i, a) };
            let ahead = if self_loop { None } else { run_to_end(i, b) };
            let (start, entry, closed) = match (back, ahead) {
                (Some(x), Some(y)) => {
                    let (s, e) = if y.0 < x.0 { y } else { x };
                    (s, e, false)
                }
                _ => (i, a, true),
            };
            let mut items = Vec::new();
            let (mut cur, mut node) = (start, entry);
            loop {
                seen[cur] = true;
                let [first, _] = links[cur].expect("offene Kurve");
                items.push((ids[cur], first != node));
                node = far(cur, node);
                match other(cur, node) {
                    Some(o) if o != start && !seen[o] => cur = o,
                    _ => break,
                }
            }
            chains.push(OrderedChain { items, closed });
        }
        Ok(chains)
    }

    /// Welche Kurven zählen: alle, außer Konstruktionskurven in einem Zusammenhang, in dem auch
    /// eine normale Kurve liegt.
    fn without_construction_rim(
        &self,
        ids: &[CurveId],
        links: &[Option<[usize; 2]>],
        node_count: usize,
    ) -> Vec<bool> {
        let mut parent: Vec<usize> = (0..node_count).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for [a, b] in links.iter().flatten() {
            let (x, y) = (find(&mut parent, *a), find(&mut parent, *b));
            parent[x] = y;
        }
        let construction = |i: usize| self.curve(ids[i]).is_none_or(|c| c.construction);
        let mut has_normal = vec![false; node_count];
        for (i, l) in links.iter().enumerate() {
            if let (false, Some([a, _])) = (construction(i), l) {
                let root = find(&mut parent, *a);
                has_normal[root] = true;
            }
        }
        links
            .iter()
            .enumerate()
            .map(|(i, l)| match l {
                Some([a, _]) if construction(i) => !has_normal[find(&mut parent, *a)],
                _ => true,
            })
            .collect()
    }

    /// Abstand des Zeigers zur Kette mit Vorzeichen, positiv links der Kettenrichtung. `None`,
    /// wenn er auf der Kette liegt oder die Seite nicht bestimmt ist.
    pub fn offset_side(&self, chain: &OrderedChain, pointer: DVec2) -> Option<f64> {
        if !pointer.is_finite() {
            return None;
        }
        let geoms: Vec<Geom> = chain
            .items
            .iter()
            .map(|(c, _)| self.geom(*c))
            .collect::<Option<_>>()?;
        signed_distance(&geoms, chain, pointer, self.tolerance().linear)
    }

    /// Der Plan des Versatzes um `left_distance` (positiv links der Kettenrichtung). Mehrere
    /// Ketten werden zusammen getrimmt.
    pub fn offset_plan(
        &self,
        chains: &[OrderedChain],
        left_distance: f64,
    ) -> Result<OffsetPlan, OffsetProblem> {
        let tolerance = *self.tolerance();
        let tol = tolerance.linear;
        let d = left_distance;
        if !d.is_finite() || d.abs() <= tol || chains.is_empty() {
            return Err(OffsetProblem::Degenerate);
        }
        let mut templates = Vec::new();
        let mut pieces = Vec::new();
        let mut kites = Vec::new();
        let mut caps = Vec::new();
        let mut closed_sources = Vec::new();
        let mut chain_geoms = Vec::with_capacity(chains.len());
        for (index, chain) in chains.iter().enumerate() {
            if chain.items.is_empty() {
                return Err(OffsetProblem::Degenerate);
            }
            let mut geoms = Vec::with_capacity(chain.items.len());
            let mut raws = Vec::with_capacity(chain.items.len());
            for &(c, against) in &chain.items {
                let g = self.geom(c).ok_or(OffsetProblem::Degenerate)?;
                raws.extend(self.raw_pieces(c, against, &g, d, tol)?);
                templates.push(Template {
                    source: c,
                    bbox: g.bbox(),
                    geom: g.clone(),
                });
                geoms.push(g);
                if chain.closed {
                    closed_sources.push(c);
                }
            }
            if !chain.closed {
                let end_of = |item: (CurveId, bool), at_end: bool| {
                    let [a, b] = open_ends(self, item.0).ok_or(OffsetProblem::Degenerate)?;
                    Ok(if at_end != item.1 { b } else { a })
                };
                caps.push(end_of(chain.items[0], false)?);
                caps.push(end_of(chain.items[chain.items.len() - 1], true)?);
            }
            chain_geoms.push(geoms);
            settle_chain(
                &raws,
                chain.closed,
                index,
                d,
                &tolerance,
                &mut pieces,
                &mut kites,
            );
        }
        if pieces.is_empty() {
            return Err(OffsetProblem::Vanished);
        }
        let loops = trim(&TrimInput {
            pieces: &pieces,
            templates: &templates,
            kites: &kites,
            caps: &caps,
            chains,
            chain_geoms: &chain_geoms,
            d,
            tol,
        })?;
        let all: HashSet<CurveId> = templates.iter().map(|t| t.source).collect();
        let kept: HashSet<CurveId> = loops
            .iter()
            .flat_map(|l| l.pieces.iter().map(|p| p.source))
            .collect();
        Ok(OffsetPlan {
            loops,
            distance: d,
            dropped: all.len() - kept.len(),
            closed_sources,
        })
    }
}
