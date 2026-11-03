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
//! mehr trifft, entfällt, wenn er selbst zu nah an der Vorlage liegt; liegt sein Nachbar zu nah,
//! entfällt der. Bleibt ein Bogen über 180° so an beiden Enden ohne Nachbarn und passt sein ganzer
//! Kreis, ist er eine eigene Schleife (Schlüsselloch). Nach außen verschwindet nie etwas: Bleibt
//! dort nichts, heißt das `NoJoin`.
//!
//! **Abhängigkeiten** (`add_offset`): Die Kopie bekommt keine neue Art von Zeile. Ein
//! Konstruktionskreis um einen Punkt der Vorlage (Rollkreis), der die Kopie berührt, hält den
//! Abstand; der Zweig der Tangente steht beim Anlegen fest, also wechselt die Seite nie. Einen
//! Rollkreis gibt es je Lauf (Stücke zwischen zwei scharfen Fugen), alle gleich groß über `Equal`,
//! am ersten das Maß. Linien tragen `Parallel`, Bögen und Kreise teilen die Mitte der Vorlage,
//! tangentiale Fugen `Tangent`, offene Enden eine Querlinie. Näherungen tragen nichts und folgen
//! nicht.

use crate::arrangement::{Nodes, polygon_contains, split};
use crate::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use crate::curves::Sampled;
use crate::error::SketchError;
use crate::fillet::{Shape, meet, wrap_pi};
use crate::geom::Geom;
use crate::intersect::{Prepared, boxes_touch, intersect, intersect_prepared, self_intersect};
use crate::model::{CurveId, CurveKind, MAX_COORD, PointId, Sketch};
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
/// Ein Bogen schließt sich zum Kreis, wenn sein fehlender Teil höchstens so viel weiter als `|d|`
/// von der Vorlage liegt (relativ), geprüft an so vielen Stellen.
const ISLAND_SEAM: f64 = 0.05;
const ISLAND_SAMPLES: usize = 8;
/// `1 − κ·d` ab hier gilt als Spitze.
const CUSP_MARGIN: f64 = 1e-9;
/// Bis zu so vielen Stücken bekommt eine Kopie Abhängigkeiten. Jedes Lösen rechnet den Rang der
/// ganzen Gruppe dicht; darüber entsteht sie als reine Geometrie mit gemeinsamen Punkten.
pub const OFFSET_TIED_MAX: usize = 64;
/// Sinus, bis zu dem eine Linie am Rollkreis als parallel zur Vorlage gilt.
const ROLLER_PARALLEL: f64 = 1e-6;

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
    /// Ein Stück `Circle` trägt keine Richtung und läuft immer gegen den Uhrzeigersinn.
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

/// Was [`Sketch::add_offset`] angelegt hat.
#[derive(Clone, Debug, PartialEq)]
pub struct OffsetIds {
    /// Die Stücke der Kopie, in der Reihenfolge des Plans.
    pub curves: Vec<CurveId>,
    pub rollers: Vec<CurveId>,
    /// Das Maß des Abstands, der Radius am ersten Rollkreis.
    pub distance_dim: Option<ConstraintId>,
    /// Die Kopie folgt der Vorlage: Es gibt mindestens einen Rollkreis.
    pub tied: bool,
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

/// Durchläuft die Schleife den Bogen, ihr Stück `i`, gegen seine Richtung? `prev`: wo der Lauf
/// gerade steht. Am Anfang der Schleife entscheidet das nächste Stück.
fn arc_runs_backwards(arc: &ArcPlan, prev: Option<DVec2>, l: &OffsetLoop, i: usize) -> bool {
    let n = l.pieces.len();
    match prev {
        Some(prev) => arc.start != prev && arc.end == prev,
        None if n > 1 && (l.closed || i + 1 < n) => {
            let next = start_points(&l.pieces[(i + 1) % n]);
            !next.contains(&arc.end) && next.contains(&arc.start)
        }
        None => false,
    }
}

fn loop_outline(l: &OffsetLoop, steps: usize) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::new();
    for (i, p) in l.pieces.iter().enumerate() {
        let mut pl = piece_polyline(p, steps);
        if let OffsetGeom::Arc(arc) = &p.geom
            && arc_runs_backwards(arc, out.last().copied(), l, i)
        {
            pl.reverse();
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

/// Die offenen Enden einer Kurve (Kreis und Ellipse haben keine).
pub(crate) fn ends_of(kind: &CurveKind) -> Vec<PointId> {
    match kind {
        CurveKind::Line { a, b } => vec![*a, *b],
        CurveKind::Arc { start, end, .. } | CurveKind::EllipticArc { start, end, .. } => {
            vec![*start, *end]
        }
        CurveKind::Spline { points } => vec![points[0], points[points.len() - 1]],
        CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => Vec::new(),
    }
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
        let finish = |kind: RawKind| {
            let ends = open_ends(self, c).ok_or(BAD)?;
            let (from, to) = if against {
                (ends[1], ends[0])
            } else {
                (ends[0], ends[1])
            };
            let (t_from, t_to) = if against { (hi, lo) } else { (lo, hi) };
            let dir_from = unit_tangent(g, t_from, against).ok_or(BAD)?;
            let dir_to = unit_tangent(g, t_to, against).ok_or(BAD)?;
            Ok(Raw {
                source: c,
                kind,
                against,
                from,
                to,
                dir_from,
                dir_to,
                raw_from: from + dir_from.perp() * d,
                raw_to: to + dir_to.perp() * d,
            })
        };
        match g {
            Geom::Line { .. } => Ok(vec![finish(RawKind::Line)?]),
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
                Ok(vec![finish(kind)?])
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
    /// Enden in Laufrichtung.
    start: DVec2,
    end: DVec2,
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
            start: js,
            end: je,
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
            // Reichen die Fugen um den ganzen Kreis, läuft der Bogen über sich selbst: ein Kreis, den
            // die Nachbarn schneiden.
            let geom = if span >= TAU {
                Geom::Circle { c: *c, r: *r }
            } else {
                Geom::Arc {
                    c: *c,
                    r: *r,
                    a0: t0,
                    sweep: span,
                }
            };
            Cut {
                start: js,
                end: je,
                geom,
                points: None,
                length: span * r,
                full: sweep * r,
            }
        }
        RawKind::Whole { c, r } => Cut {
            start: js,
            end: je,
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
        start: js,
        end: je,
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
    /// Die beiden Vorlagen der Fuge. Ihre Stücke zählen hier nicht: Bei einem Bogen ist der Rand
    /// der Gehrung nur seine Sehne, und der Bogen läge scheinbar in seiner eigenen Gehrung.
    owners: [CurveId; 2],
}

impl Kite {
    fn new(points: [DVec2; 5], owners: [CurveId; 2]) -> Kite {
        let bbox = points
            .iter()
            .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
        Kite {
            points,
            bbox,
            owners,
        }
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
        sum += match raws[next].kind {
            // Über 180° trägt der Winkel zwischen den Tangenten nicht.
            RawKind::Round { sweep, .. } if raws[next].against => -sweep,
            RawKind::Round { sweep, .. } => sweep,
            _ => angle(raws[next].dir_from, raws[next].dir_to),
        };
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
    (closed, chain, geoms): (bool, usize, &[Geom]),
    d: f64,
    tol: &Tolerance,
    out: &mut Vec<Settled>,
    kites: &mut Vec<Kite>,
) {
    let n = raws.len();
    let mut alive: Vec<usize> = (0..n).collect();
    let mut islands: Vec<usize> = Vec::new();
    loop {
        let m = alive.len();
        if m == 0 {
            push_islands(&islands, raws, chain, out);
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
        // Trifft ein kleiner werdender Bogen seinen Nachbarn nicht mehr, entfällt, wer zu nah an
        // der Vorlage liegt: der Nachbar oder der Bogen. Liegen beide richtig, entfällt keiner.
        for (k, join) in joins.iter().enumerate() {
            if *join != Some(Join::Gap) {
                continue;
            }
            let next = (k + 1) % m;
            let shrinks =
                |i: usize| matches!(raws[alive[i]].kind, RawKind::Round { shrinks: true, .. });
            if !shrinks(k) && !shrinks(next) {
                continue;
            }
            for i in [k, next] {
                if too_close_to_chain(&cuts[i].geom, geoms, d, tol.linear) {
                    collapse[i] = Some(collapse[i].unwrap_or(d.abs()));
                }
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
        // Ein Bogen ohne Nachbarn an beiden Enden, dessen Kreis passt, schließt sich zum Kreis und
        // ist eine eigene Schleife; seine Nachbarn werden neu gefügt.
        if closed {
            let island = (0..m).find(|&k| {
                joins[k] == Some(Join::Gap)
                    && before(k) == Some(Join::Gap)
                    && circle_fits(&raws[alive[k]], geoms, d, tol.linear)
            });
            if let Some(k) = island {
                islands.push(alive.remove(k));
                continue;
            }
        }
        let base = out.len();
        let m_raw = cuts.into_iter().enumerate();
        for (k, c) in m_raw {
            let raw = &raws[alive[k]];
            let before_join = before(k);
            let after_join = joins[k];
            let next = base + (k + 1) % m;
            out.push(Settled {
                source: raw.source,
                chain,
                // Linie und Näherung sind schon in Laufrichtung gebaut.
                against: raw.against
                    && matches!(raw.kind, RawKind::Round { .. } | RawKind::Whole { .. }),
                geom: c.geom,
                points: c.points,
                start: c.start,
                end: c.end,
                gap_start: before_join == Some(Join::Gap),
                gap_end: after_join == Some(Join::Gap),
                tangent_next: matches!(after_join, Some(Join::Shared(_))).then_some(next),
            });
            if let Some(Join::Miter(x)) = after_join {
                let q = &raws[alive[(k + 1) % m]];
                let (i, j) = (alive[k], alive[(k + 1) % m]);
                if d * turn_between(raws, i, j) < 0.0 {
                    kites.push(Kite::new(
                        [raw.to, raw.raw_to, x, q.raw_from, q.from],
                        [raw.source, q.source],
                    ));
                }
            }
        }
        push_islands(&islands, raws, chain, out);
        return;
    }
}

/// Liegt die Mitte des Stücks näher als `|d|` an der Kette?
fn too_close_to_chain(g: &Geom, geoms: &[Geom], d: f64, tol: f64) -> bool {
    let (lo, hi) = g.domain();
    let mid = g.eval(0.5 * (lo + hi)).0;
    let limit = d.abs() - SLACK * tol;
    geoms
        .iter()
        .any(|t| t.eval(t.nearest(mid)).0.distance(mid) < limit)
}

/// Passt der ganze Kreis eines Bogens? Der fehlende Teil muss überall im Abstand `|d|` liegen,
/// bis auf den schmalen Saum neben einer Öffnung. Liegt er weiter weg, ist die Fläche dort größer
/// als der Kreis, und das Ergebnis wäre unvollständig.
fn circle_fits(raw: &Raw, geoms: &[Geom], d: f64, tol: f64) -> bool {
    let RawKind::Round {
        c,
        r,
        a0,
        sweep,
        shrinks: true,
    } = raw.kind
    else {
        return false;
    };
    // Bei einem kleinen Bogen sagt der Abstand nichts: Nahe seiner Mitte liegt alles gleich weit.
    if sweep < PI {
        return false;
    }
    let (near, far) = (d.abs() - SLACK * tol, d.abs() * (1.0 + ISLAND_SEAM));
    (1..ISLAND_SAMPLES).all(|k| {
        let a = a0 + sweep + (TAU - sweep) * k as f64 / ISLAND_SAMPLES as f64;
        let p = c + DVec2::from_angle(a) * r;
        let dist = geoms
            .iter()
            .map(|t| t.eval(t.nearest(p)).0.distance(p))
            .fold(f64::INFINITY, f64::min);
        (near..=far).contains(&dist)
    })
}

/// Die zum Kreis geschlossenen Bögen als eigene Stücke.
fn push_islands(islands: &[usize], raws: &[Raw], chain: usize, out: &mut Vec<Settled>) {
    for &i in islands {
        let raw = &raws[i];
        let RawKind::Round { c, r, .. } = raw.kind else {
            continue;
        };
        let at = c + DVec2::new(r, 0.0);
        out.push(Settled {
            source: raw.source,
            chain,
            against: raw.against,
            geom: Geom::Circle { c, r },
            points: None,
            start: at,
            end: at,
            gap_start: false,
            gap_end: false,
            tangent_next: None,
        });
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
            if valid_mid(input, check, own, p) {
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
fn valid_mid(input: &TrimInput, p: DVec2, skip_own: bool, piece: &Settled) -> bool {
    let chain = piece.chain;
    let skip = skip_own.then_some(piece.source);
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
    !close
        && !input
            .kites
            .iter()
            .any(|k| !k.owners.contains(&piece.source) && k.contains(p, SLACK * input.tol))
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
            Geom::Circle { c, r } | Geom::Arc { c, r, .. } if from == to => OffsetGeom::Circle {
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

/// Führt der Versatz nach links um `d` aus der geschlossenen Kette hinaus?
fn runs_outward(chain: &OrderedChain, geoms: &[Geom], d: f64) -> bool {
    let mut area = 0.0;
    let mut last: Option<DVec2> = None;
    for (g, (_, against)) in geoms.iter().zip(&chain.items) {
        let (lo, hi) = g.domain();
        let (t0, t1) = if *against { (hi, lo) } else { (lo, hi) };
        for q in g.polyline(t0, t1, 64) {
            if let Some(p) = last {
                area += p.perp_dot(q);
            }
            last = Some(q);
        }
    }
    area * d < 0.0
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
            let geoms_of_chain = geoms.clone();
            chain_geoms.push(geoms);
            settle_chain(
                &raws,
                (chain.closed, index, geoms_of_chain.as_slice()),
                d,
                &tolerance,
                &mut pieces,
                &mut kites,
            );
        }
        // Nach außen ist immer Platz: Verschwindet dort alles, fehlt nur die Verbindung.
        let vanished = if chains
            .iter()
            .zip(&chain_geoms)
            .any(|(c, g)| c.closed && runs_outward(c, g, d))
        {
            OffsetProblem::NoJoin
        } else {
            OffsetProblem::Vanished
        };
        if pieces.is_empty() {
            return Err(vanished);
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
        })
        .map_err(|e| {
            if e == OffsetProblem::Vanished {
                vanished
            } else {
                e
            }
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

// ---------------------------------------------------------------------------------------------
// Abhängigkeiten
// ---------------------------------------------------------------------------------------------

/// Anfang und Ende jedes Stücks im Lauf der Schleife.
fn travel(l: &OffsetLoop) -> Vec<(DVec2, DVec2)> {
    let mut out: Vec<(DVec2, DVec2)> = Vec::with_capacity(l.pieces.len());
    for (i, p) in l.pieces.iter().enumerate() {
        let way = match &p.geom {
            OffsetGeom::Line { a, b } => (*a, *b),
            OffsetGeom::Arc(arc) if arc_runs_backwards(arc, out.last().map(|w| w.1), l, i) => {
                (arc.end, arc.start)
            }
            OffsetGeom::Arc(arc) => (arc.start, arc.end),
            OffsetGeom::Circle { center, radius } => {
                let at = *center + DVec2::new(*radius, 0.0);
                (at, at)
            }
            OffsetGeom::Approx { points } => (points[0], points[points.len() - 1]),
        };
        out.push(way);
    }
    out
}

/// Einheitsrichtung, in der das Stück den Endpunkt `at` verlässt. Näherungen und Kreise: `None`,
/// sie haben keine tangentiale Fuge mit Abhängigkeit.
fn leaving(geom: &OffsetGeom, at: DVec2) -> Option<DVec2> {
    match geom {
        OffsetGeom::Line { a, b } => (if *a == at { *b - *a } else { *a - *b }).try_normalize(),
        OffsetGeom::Arc(arc) => {
            let radial = (at - arc.center).try_normalize()?;
            Some(if arc.start == at {
                radial.perp()
            } else {
                -radial.perp()
            })
        }
        OffsetGeom::Circle { .. } | OffsetGeom::Approx { .. } => None,
    }
}

/// Läuft die Schleife von `p` in `at` tangential nach `q` weiter? Dieselbe Regel wie beim Fügen.
fn tangent_joint(p: &OffsetGeom, q: &OffsetGeom, at: DVec2, d: f64, tol: &Tolerance) -> bool {
    let (Some(u), Some(v)) = (leaving(p, at), leaving(q, at)) else {
        return false;
    };
    let sine = u.perp_dot(v).abs();
    u.dot(v) < 0.0 && (sine <= tol.angular || sine * d.abs() <= tol.linear)
}

/// Zwei Linien auf einer Geraden, die sich in `at` tangential treffen.
fn is_bridge(p: &OffsetPiece, q: &OffsetPiece, at: DVec2, d: f64, tol: &Tolerance) -> bool {
    matches!(
        (&p.geom, &q.geom),
        (OffsetGeom::Line { .. }, OffsetGeom::Line { .. })
    ) && tangent_joint(&p.geom, &q.geom, at, d, tol)
}

/// Liegen die Stücke auf demselben Träger (Linien auf einer Geraden, Bögen um dieselbe Mitte)?
/// Dann hält keine Zeile die Fuge an ihrem Platz: Sie kann auf dem Träger gleiten.
fn same_carrier(p: &OffsetGeom, q: &OffsetGeom) -> bool {
    match (p, q) {
        (OffsetGeom::Line { .. }, OffsetGeom::Line { .. }) => true,
        (OffsetGeom::Arc(x), OffsetGeom::Arc(y)) => x.center == y.center,
        _ => false,
    }
}

/// Punkte der Fugen: Gleiche Knoten sind bitgleich, also genügt die Lage als Schlüssel.
#[derive(Default)]
struct Joints(HashMap<(u64, u64), PointId>);

impl Joints {
    /// Die Fuge an der Lage `at` des Plans ist schon angelegt, als `id`.
    fn seed(&mut self, at: DVec2, id: PointId) {
        self.0.insert((at.x.to_bits(), at.y.to_bits()), id);
    }

    fn at(&mut self, s: &mut Sketch, p: DVec2) -> Result<PointId, SketchError> {
        let key = (p.x.to_bits(), p.y.to_bits());
        if let Some(&id) = self.0.get(&key) {
            return Ok(id);
        }
        let id = s.add_point(p.x, p.y)?;
        self.0.insert(key, id);
        Ok(id)
    }
}

impl Sketch {
    /// Mitte des Bogens oder Kreises `c`; sie muss noch an der Lage `at` des Plans stehen.
    fn offset_center(&self, c: CurveId, at: DVec2) -> Result<PointId, SketchError> {
        match self.curves[c].kind {
            CurveKind::Arc { center, .. } | CurveKind::Circle { center, .. }
                if self.pos(center).distance(at) <= self.tolerance().linear =>
            {
                Ok(center)
            }
            _ => Err(SketchError::UnknownObject),
        }
    }

    /// Legt ein Stück an. Eine gebundene Kopie teilt die Mitte mit der Vorlage, eine freie nicht.
    fn add_offset_piece(
        &mut self,
        piece: &OffsetPiece,
        tied: bool,
        joints: &mut Joints,
    ) -> Result<CurveId, SketchError> {
        let center = |s: &mut Sketch, at: DVec2| {
            if tied {
                s.offset_center(piece.source, at)
            } else {
                s.add_point(at.x, at.y)
            }
        };
        let curve = match &piece.geom {
            OffsetGeom::Line { a, b } => {
                let (a, b) = (joints.at(self, *a)?, joints.at(self, *b)?);
                self.add_line(a, b)?
            }
            OffsetGeom::Arc(arc) => {
                let m = center(self, arc.center)?;
                let (start, end) = (joints.at(self, arc.start)?, joints.at(self, arc.end)?);
                self.add_arc(m, start, end)?
            }
            OffsetGeom::Circle { center: c, radius } => {
                let m = center(self, *c)?;
                self.add_circle(m, *radius)?
            }
            OffsetGeom::Approx { points } => {
                let last = points.len() - 1;
                let mut ids = Vec::with_capacity(points.len());
                for (k, p) in points.iter().enumerate() {
                    ids.push(if k == 0 || k == last {
                        joints.at(self, *p)?
                    } else {
                        self.add_point(p.x, p.y)?
                    });
                }
                self.add_spline(&ids)?
            }
        };
        // Stil wie die Vorlage; die Kopie einer festen Kurve (projizierte Kante) ist normal.
        let source = &self.curves[piece.source];
        let (construction, centerline) = (source.construction, source.centerline);
        if !self.curve_has_fixed_point(piece.source) {
            self.set_construction(curve, construction)?;
            if centerline {
                self.set_centerline(curve, true)?;
            }
        }
        Ok(curve)
    }

    fn curve_has_fixed_point(&self, c: CurveId) -> bool {
        let points = self.curves[c].kind.points();
        self.constraints()
            .any(|(_, d)| matches!(d.kind, ConstraintKind::Fixed(p, ..) if points.contains(&p)))
    }

    /// Der Punkt der Vorlage `source`, um den der Rollkreis läuft. Am Kreis ein neuer
    /// Konstruktionspunkt, denn dort gibt es keinen.
    fn roller_center(
        &mut self,
        source: CurveId,
        created: &mut Vec<ConstraintId>,
    ) -> Result<PointId, SketchError> {
        match self.curves[source].kind {
            CurveKind::Line { a, .. } => Ok(a),
            CurveKind::Arc { start, .. } => Ok(start),
            CurveKind::Circle { center, radius } => {
                let m = self.pos(center);
                let helper = self.add_point(m.x + self.vars[radius], m.y)?;
                self.points[helper].construction = true;
                created.push(self.add_constraint(ConstraintKind::Horizontal(
                    LineOrPoints::Points(center, helper),
                ))?);
                created.push(self.add_constraint(ConstraintKind::PointOnCurve(helper, source))?);
                Ok(helper)
            }
            _ => Err(SketchError::UnknownObject),
        }
    }

    /// Querlinie am offenen Ende oder an der gleitenden Fuge `at` der Kopie, wenn sie genau
    /// gegenüber einem Ende der Vorlage liegt, und ob es sie gibt. Ein vom Trimmen gekürztes Ende
    /// bleibt frei: Dort gäbe es keine Lage ohne Bewegung.
    fn add_cross_line(
        &mut self,
        source: CurveId,
        at: DVec2,
        distance: f64,
        joints: &mut Joints,
        created: &mut Vec<ConstraintId>,
    ) -> Result<bool, SketchError> {
        let tol = self.tolerance().linear;
        let joint = joints.at(self, at)?;
        match self.curves[source].kind {
            CurveKind::Line { .. } => {
                let Some(q) = self.opposite_line_end(source, at, distance) else {
                    return Ok(false);
                };
                let line = self.fillet_helper_line(q, joint)?;
                created.push(self.add_constraint(ConstraintKind::Perpendicular(source, line))?);
            }
            CurveKind::Arc { center, start, end } => {
                let m = self.pos(center);
                let opposite = [start, end].into_iter().find(|&q| {
                    (self.pos(q) - m).try_normalize().is_some_and(|dir| {
                        let v = at - m;
                        v.dot(dir) > 0.0 && dir.perp_dot(v).abs() <= tol
                    })
                });
                let Some(q) = opposite else {
                    return Ok(false);
                };
                let line = self.fillet_helper_line(center, q)?;
                created.push(self.add_constraint(ConstraintKind::PointOnCurve(joint, line))?);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Das Ende der Linie `source`, dem `at` im Abstand `distance` genau gegenüberliegt.
    fn opposite_line_end(&self, source: CurveId, at: DVec2, distance: f64) -> Option<PointId> {
        let tol = self.tolerance().linear;
        let CurveKind::Line { a, b } = self.curves[source].kind else {
            return None;
        };
        let dir = (self.pos(b) - self.pos(a)).try_normalize()?;
        [a, b].into_iter().find(|&q| {
            let v = at - self.pos(q);
            v.dot(dir).abs() <= tol && (v.length() - distance).abs() <= tol
        })
    }

    /// Eine Fuge zweier Linien auf einer Geraden, zwischen denen Stücke entfallen sind, liegt
    /// nirgends gegenüber der Vorlage. Der Plan setzt sie irgendwo auf die Gerade, und schon beim
    /// nächsten Maß läge sie hinter einem der anderen Enden. Sie kommt in die Mitte der beiden
    /// Linien, an der Lage der Kurven ändert das nichts, und `Equal` hält sie dort.
    fn seed_bridge_joints(
        &mut self,
        l: &OffsetLoop,
        distance: f64,
        joints: &mut Joints,
    ) -> Result<(), SketchError> {
        let n = l.pieces.len();
        let way = travel(l);
        let tolerance = *self.tolerance();
        for i in 0..n {
            let j = (i + 1) % n;
            let (p, q) = (&l.pieces[i], &l.pieces[j]);
            if (l.closed || i + 1 < n)
                && i != j
                && is_bridge(p, q, way[i].1, distance, &tolerance)
                && self
                    .opposite_line_end(p.source, way[i].1, distance)
                    .is_none()
                && self
                    .opposite_line_end(q.source, way[i].1, distance)
                    .is_none()
            {
                let mid = (way[i].0 + way[j].1) * 0.5;
                let id = self.add_point(mid.x, mid.y)?;
                joints.seed(way[i].1, id);
            }
        }
        Ok(())
    }

    /// Legt den Versatz aus `plan` an (Entscheidung 3, 5, 7 und 8 des Plans): erst alle Kurven,
    /// dann `Parallel`, `Tangent`, Rollkreise und Querlinien. Was dabei überzählig ist (bei
    /// rundum tangentialer Kette eine Zeile), wird wieder entfernt. Die Werte der Skizze bleiben
    /// dabei unberührt. Über [`OFFSET_TIED_MAX`] Stücken nur die Kurven. Bei einem Fehler bleibt
    /// die Skizze unverändert.
    pub fn add_offset(&mut self, plan: &OffsetPlan) -> Result<OffsetIds, SketchError> {
        self.atomically(|s| {
            let tolerance = *s.tolerance();
            let tied = plan.piece_count() <= OFFSET_TIED_MAX;
            let mut joints = Joints::default();
            let mut curves = Vec::with_capacity(plan.piece_count());
            let mut built = Vec::with_capacity(plan.loops.len());
            for l in &plan.loops {
                for piece in &l.pieces {
                    s.check_curve(piece.source)?;
                }
                if tied {
                    s.seed_bridge_joints(l, plan.distance.abs(), &mut joints)?;
                }
                let mut ids = Vec::with_capacity(l.pieces.len());
                for piece in &l.pieces {
                    ids.push(s.add_offset_piece(piece, tied, &mut joints)?);
                }
                curves.extend_from_slice(&ids);
                built.push(ids);
            }
            if !tied {
                return Ok(OffsetIds {
                    curves,
                    rollers: Vec::new(),
                    distance_dim: None,
                    tied: false,
                });
            }

            let mut created = Vec::new();
            for (l, ids) in plan.loops.iter().zip(&built) {
                for (piece, &k) in l.pieces.iter().zip(ids) {
                    if matches!(piece.geom, OffsetGeom::Line { .. }) {
                        created.push(s.add_constraint(ConstraintKind::Parallel(piece.source, k))?);
                    }
                }
            }
            // Je Schleife die Stücke, die einen Lauf beginnen, und die Fugen, die gleiten könnten.
            let mut leads = Vec::with_capacity(built.len());
            let mut sliding = Vec::new();
            for (l, ids) in plan.loops.iter().zip(&built) {
                let n = l.pieces.len();
                let way = travel(l);
                let tangent: Vec<bool> = (0..n)
                    .map(|i| {
                        (l.closed || i + 1 < n)
                            && tangent_joint(
                                &l.pieces[i].geom,
                                &l.pieces[(i + 1) % n].geom,
                                way[i].1,
                                plan.distance,
                                &tolerance,
                            )
                    })
                    .collect();
                for i in (0..n).filter(|&i| tangent[i]) {
                    if same_carrier(&l.pieces[i].geom, &l.pieces[(i + 1) % n].geom) {
                        sliding.push((
                            &l.pieces[i],
                            &l.pieces[(i + 1) % n],
                            ids[i],
                            ids[(i + 1) % n],
                            way[i].1,
                        ));
                    } else {
                        let kind = ConstraintKind::Tangent(ids[i], ids[(i + 1) % n]);
                        created.push(s.add_constraint(kind)?);
                    }
                }
                let mut starts: Vec<usize> =
                    (0..n).filter(|&i| !tangent[(i + n - 1) % n]).collect();
                if starts.is_empty() {
                    starts.push(0);
                }
                leads.push(starts);
            }

            let mut rollers: Vec<CurveId> = Vec::new();
            let mut distance_dim = None;
            let rho = plan.distance.abs();
            for ((l, ids), starts) in plan.loops.iter().zip(&built).zip(&leads) {
                for &i in starts {
                    let piece = &l.pieces[i];
                    if matches!(piece.geom, OffsetGeom::Approx { .. }) {
                        continue;
                    }
                    let center = s.roller_center(piece.source, &mut created)?;
                    let roller = s.add_circle(center, rho)?;
                    s.set_construction(roller, true)?;
                    match rollers.first() {
                        None => {
                            distance_dim =
                                Some(s.add_constraint(ConstraintKind::Radius(roller, rho))?);
                        }
                        Some(&first) => {
                            created.push(s.add_constraint(ConstraintKind::Equal(first, roller))?);
                        }
                    }
                    let touch = if matches!(piece.geom, OffsetGeom::Line { .. }) {
                        ConstraintKind::Tangent(ids[i], roller)
                    } else {
                        ConstraintKind::Tangent(roller, ids[i])
                    };
                    created.push(s.add_constraint(touch)?);
                    rollers.push(roller);
                }
            }
            for l in plan.loops.iter().filter(|l| !l.closed) {
                let way = travel(l);
                let (first, last) = (&l.pieces[0], &l.pieces[l.pieces.len() - 1]);
                s.add_cross_line(first.source, way[0].0, rho, &mut joints, &mut created)?;
                let end = way[way.len() - 1].1;
                s.add_cross_line(last.source, end, rho, &mut joints, &mut created)?;
            }
            for (p, q, x, y, at) in sliding {
                if !s.add_cross_line(p.source, at, rho, &mut joints, &mut created)?
                    && !s.add_cross_line(q.source, at, rho, &mut joints, &mut created)?
                    && is_bridge(p, q, at, plan.distance, &tolerance)
                {
                    created.push(s.add_constraint(ConstraintKind::Equal(x, y))?);
                }
            }

            // Auf einer Kopie gelöst: Hier soll sich nichts bewegen.
            let report = s.clone().solve();
            for id in report.redundant {
                if created.contains(&id) {
                    s.remove_constraint(id)?;
                }
            }
            let tied = !rollers.is_empty();
            Ok(OffsetIds {
                curves,
                rollers,
                distance_dim,
                tied,
            })
        })
    }

    /// Sitzt der Rollkreis `roller` so auf der Vorlage, wie `add_offset` ihn anlegt, und berührt er
    /// eine ihrer Kopien (eine aus `touched`)? Linie: Die Mitte ist Ende einer Linie, zu der die
    /// Berührte parallel ist. Rund: Die Mitte ist Anfang eines Bogens oder liegt auf einem Kreis,
    /// und die Berührte teilt dessen Mitte. So bleibt ein von Hand gezeichneter Kreis, der nur an
    /// einem Linienende sitzt und eine andere Linie berührt, draußen.
    pub(crate) fn rolls_on_source(&self, roller: CurveId, touched: &[CurveId]) -> bool {
        let CurveKind::Circle { center: s, .. } = self.curves[roller].kind else {
            return false;
        };
        let on_circle = |q: CurveId| {
            self.constraints().any(|(_, d)| {
                d.enabled
                    && matches!(d.kind, ConstraintKind::PointOnCurve(p, c) if p == s && c == q)
            })
        };
        touched.iter().any(|&k| match self.curves[k].kind {
            CurveKind::Line { a, b } => {
                let dir = self.pos(b) - self.pos(a);
                self.curves().any(|(q, d)| match d.kind {
                    CurveKind::Line { a: x, b: y } if q != k && (x == s || y == s) => {
                        let sine = (self.pos(y) - self.pos(x))
                            .normalize_or_zero()
                            .perp_dot(dir.normalize_or_zero());
                        sine.abs() <= ROLLER_PARALLEL
                            || self
                                .constraints()
                                .any(|(_, c)| c.enabled && c.kind == ConstraintKind::Parallel(q, k))
                    }
                    _ => false,
                })
            }
            CurveKind::Arc { center: m, .. } | CurveKind::Circle { center: m, .. } => {
                self.curves().any(|(q, d)| match d.kind {
                    CurveKind::Arc { center, start, .. } => q != k && center == m && start == s,
                    CurveKind::Circle { center, .. } => q != k && center == m && on_circle(q),
                    _ => false,
                })
            }
            _ => false,
        })
    }

    /// Ist `c`, das ein Ende mit dem Stück `x` der Kopie teilt, ein weiteres Stück von ihr? Es
    /// trägt `Tangent` oder `Equal` zu `x` oder `Parallel` zu einer Kurve, die es nicht berührt,
    /// teilt als Bogen die Mitte mit `x`, oder ist eine Näherung (die trägt gar nichts).
    pub(crate) fn continues_copy(&self, x: CurveId, c: CurveId) -> bool {
        let points = self.curves[c].kind.points();
        self.joins_copy(x, c)
            || self.constraints().any(|(_, d)| {
                d.enabled
                    && matches!(d.kind, ConstraintKind::Parallel(q, k) if k == c
                        && !self.curves[q].kind.points().iter().any(|p| points.contains(p)))
            })
    }

    /// Der feste Teil von [`continues_copy`](Self::continues_copy): alles außer `Parallel`, das
    /// auch eine eigene Linie des Nutzers tragen kann.
    fn joins_copy(&self, x: CurveId, c: CurveId) -> bool {
        let bound = self.constraints().any(|(_, d)| {
            d.enabled
                && matches!(d.kind, ConstraintKind::Tangent(a, b) | ConstraintKind::Equal(a, b)
                    if (a, b) == (x, c) || (a, b) == (c, x))
        });
        bound
            || match (&self.curves[x].kind, &self.curves[c].kind) {
                (CurveKind::Arc { center: m, .. }, CurveKind::Arc { center: n, .. }) => m == n,
                (_, CurveKind::Spline { .. }) => true,
                _ => false,
            }
    }

    /// Die Enden der Vorlagen von `copies`: Linien, zu denen ein Stück parallel ist, und Bögen um
    /// dieselbe Mitte. Eine Linie, die hier endet und an der Kopie hängt, schließt die Wand von Hand
    /// und ist kein Stück der Kopie, was immer sie sonst bindet.
    fn source_ends(&self, copies: &[CurveId], centers: &[PointId]) -> HashSet<PointId> {
        let mut out: HashSet<PointId> = centers.iter().copied().collect();
        for (_, d) in self.constraints().filter(|(_, d)| d.enabled) {
            if let ConstraintKind::Parallel(q, k) = d.kind
                && copies.contains(&k)
                && !copies.contains(&q)
            {
                out.extend(ends_of(&self.curves[q].kind));
            }
        }
        for &k in copies {
            if let CurveKind::Arc { center: m, .. } = self.curves[k].kind {
                for (q, d) in self.curves() {
                    if let CurveKind::Arc { center, .. } = d.kind
                        && center == m
                        && !copies.contains(&q)
                    {
                        out.extend(ends_of(&d.kind));
                    }
                }
            }
        }
        out
    }

    /// Die Punkte der Vorlage, die stehen bleiben, wenn sich das Versatzmaß `dim` ändert. `None`,
    /// wenn `dim` kein solches Maß ist: ein `Radius` an einem Konstruktionskreis, der tangential an
    /// einer Kurve hängt und auf deren Vorlage sitzt ([`Sketch::rolls_on_source`]).
    ///
    /// Das Ergebnis sind alle Punkte aller Kurven, die über gemeinsame Enden von den Mitten der
    /// ganzen `Equal`-Gruppe aus erreichbar sind, samt diesen Mitten. Gelaufen wird im Stil der
    /// Kopie (bei einer Vorlage aus Konstruktionskurven über Konstruktionskurven) und über feste
    /// Kurven, deren Kopie ja normal ist; nie über Querlinien und nie über eine Kurve, die an einem
    /// Punkt der Kopie hängt, also nie zur Kopie hinüber.
    pub fn offset_hold_points(&self, dim: ConstraintId) -> Option<Vec<PointId>> {
        self.offset_parts(dim).map(|parts| parts.hold)
    }

    /// Was der Plan zum Versatzmaß `dim` beim Wert `value` sagt: die Vorlagen aus
    /// [`offset_hold_points`](Self::offset_hold_points), die Seite je Kette von der Kopie. `None`,
    /// wenn `dim` kein Versatzmaß ist oder der Plan aufgeht.
    pub fn offset_problem_at(&self, dim: ConstraintId, value: f64) -> Option<OffsetProblem> {
        let parts = self.offset_parts(dim)?;
        let on_copy: Vec<DVec2> = parts
            .touched
            .iter()
            .flat_map(|&k| ends_of(&self.curves[k].kind))
            .map(|p| self.pos(p))
            .collect();
        let chains = match self.ordered_chains(&parts.sources) {
            Ok(chains) => chains,
            Err(problem) => return Some(problem),
        };
        chains.iter().find_map(|chain| {
            // Die Stelle der Kopie, die dieser Kette am nächsten liegt, sagt die Seite. Kreise haben
            // keine Enden; dort entscheidet ihr Radius.
            let side = on_copy
                .iter()
                .filter_map(|&p| self.offset_side(chain, p))
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                .or_else(|| self.circle_copy_side(chain, &parts.touched))?;
            let plan = self.offset_plan(std::slice::from_ref(chain), side.signum() * value);
            plan.err()
        })
    }

    /// Seite einer Kopie, die ein ganzer Kreis um die Mitte der Kette ist, wie bei `offset_side`.
    fn circle_copy_side(&self, chain: &OrderedChain, touched: &[CurveId]) -> Option<f64> {
        touched.iter().find_map(|&k| match self.curves[k].kind {
            CurveKind::Circle { center, radius } => {
                let at = self.pos(center) + DVec2::new(self.vars[radius], 0.0);
                self.offset_side(chain, at)
            }
            _ => None,
        })
    }

    /// Die Teile eines Versatzmaßes; `None` wie bei
    /// [`offset_hold_points`](Self::offset_hold_points).
    fn offset_parts(&self, dim: ConstraintId) -> Option<OffsetParts> {
        let ConstraintKind::Radius(first, _) = self.constraint(dim)?.kind else {
            return None;
        };
        let is_roller = |c: CurveId| {
            self.curve(c)
                .is_some_and(|d| d.construction && matches!(d.kind, CurveKind::Circle { .. }))
        };
        if !is_roller(first) {
            return None;
        }
        let active = || self.constraints().filter(|(_, d)| d.enabled && !d.driven);
        let mut group = vec![first];
        let mut next = 0;
        while let Some(&c) = group.get(next) {
            next += 1;
            for (_, d) in active() {
                if let ConstraintKind::Equal(x, y) = d.kind
                    && (x == c || y == c)
                {
                    let other = if x == c { y } else { x };
                    if is_roller(other) && !group.contains(&other) {
                        group.push(other);
                    }
                }
            }
        }
        let touching = |c: CurveId| -> Vec<CurveId> {
            active()
                .filter_map(|(_, d)| match d.kind {
                    ConstraintKind::Tangent(x, y) if x == c => Some(y),
                    ConstraintKind::Tangent(x, y) if y == c => Some(x),
                    _ => None,
                })
                .collect()
        };
        let centers: Vec<PointId> = group
            .iter()
            .filter_map(|&c| match self.curves[c].kind {
                CurveKind::Circle { center, .. } => Some(center),
                _ => None,
            })
            .collect();
        let mut copies: Vec<CurveId> = group.iter().flat_map(|&c| touching(c)).collect();
        let touched = copies.clone();
        if !group.iter().any(|&c| self.rolls_on_source(c, &touching(c))) {
            return None;
        }
        // Der Stil der Vorlage: normal, sobald eine Kopie normal ist.
        let construction = !copies.iter().any(|&c| !self.curves[c].construction);
        let cross_line = |c: CurveId| {
            matches!(self.curves[c].kind, CurveKind::Line { .. })
                && self.curves[c].construction
                && active().any(|(_, d)| match d.kind {
                    ConstraintKind::Perpendicular(_, x) | ConstraintKind::PointOnCurve(_, x) => {
                        x == c
                    }
                    _ => false,
                })
        };
        // Die ganze Kopie: Stücke ohne Rollkreis hängen über gemeinsame Enden an einem mit. Eine
        // eigene Linie des Nutzers zwischen Kopie und Vorlage (Wandstärke) gehört nicht dazu, auch
        // nicht, wenn sie eine Abhängigkeit trägt: Sie endet auf der Vorlage. Erst die festen Fugen,
        // denn `Parallel` kann auch zwischen zwei solchen Linien stehen und sagte sonst, was Vorlage ist.
        for firm in [true, false] {
            let mut next = 0;
            while let Some(&x) = copies.get(next) {
                next += 1;
                let ends = ends_of(&self.curves[x].kind);
                let source = self.source_ends(&copies, &centers);
                for (c, d) in self.curves() {
                    if !copies.contains(&c)
                        && !group.contains(&c)
                        && !cross_line(c)
                        && ends_of(&d.kind).iter().any(|p| ends.contains(p))
                        && !ends_of(&d.kind).iter().any(|p| source.contains(p))
                        && (if firm {
                            self.joins_copy(x, c)
                        } else {
                            self.continues_copy(x, c)
                        })
                    {
                        copies.push(c);
                    }
                }
            }
        }
        // Was an einem Punkt der Kopie hängt, gehört nicht zur Vorlage und trägt sie nicht weiter.
        let barrier: HashSet<PointId> = copies
            .iter()
            .flat_map(|&c| ends_of(&self.curves[c].kind))
            .collect();
        let walkable: Vec<(CurveId, Vec<PointId>)> = self
            .curves()
            .filter(|(c, d)| {
                (d.construction == construction || self.curve_has_fixed_point(*c))
                    && !group.contains(c)
                    && !copies.contains(c)
                    && !cross_line(*c)
            })
            .map(|(c, d)| (c, ends_of(&d.kind)))
            .filter(|(_, ends)| !ends.iter().any(|p| barrier.contains(p)))
            .collect();
        let mut reached: Vec<CurveId> = Vec::new();
        // Der Rollkreis eines Kreises sitzt auf einem Hilfspunkt, der nur auf der Vorlage liegt.
        for (_, d) in active() {
            if let ConstraintKind::PointOnCurve(p, q) = d.kind
                && centers.contains(&p)
                && (self.curves[q].construction == construction || self.curve_has_fixed_point(q))
                && matches!(self.curves[q].kind, CurveKind::Circle { .. })
            {
                reached.push(q);
            }
        }
        let mut frontier = centers.clone();
        while let Some(p) = frontier.pop() {
            for (c, ends) in &walkable {
                if ends.contains(&p) && !reached.contains(c) {
                    reached.push(*c);
                    frontier.extend(ends.iter().copied());
                }
            }
        }
        if reached.is_empty() {
            return None;
        }
        let mut hold = centers;
        for &c in &reached {
            hold.extend(self.curves[c].kind.points());
        }
        hold.sort();
        hold.dedup();
        Some(OffsetParts {
            hold,
            sources: reached,
            touched,
        })
    }
}

/// Die Teile eines Versatzmaßes.
struct OffsetParts {
    /// Punkte der Vorlage, die stehen bleiben.
    hold: Vec<PointId>,
    /// Die Vorlagekurven.
    sources: Vec<CurveId>,
    /// Die Stücke der Kopie, die einen Rollkreis berühren.
    touched: Vec<CurveId>,
}
