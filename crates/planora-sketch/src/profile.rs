//! Profil-Extraktion wie in Fusion: Jede Fläche, die von nicht-konstruktiven Kurven vollständig
//! umschlossen ist, ist ein Profil.
//!
//! Grundlage ist die ebene Anordnung aller lebenden, nicht-konstruktiven Kurven
//! (`arrangement.rs`): Kurven werden an allen Schnitt- und Berührpunkten geteilt, Stummel,
//! offene Ketten und doppelte Stücke fallen still weg. Jede beschränkte Fläche der Anordnung ist
//! ein Profil; eine Fläche innerhalb einer anderen (Kreis im Rechteck) ergibt zwei Profile, den
//! Ring (mit Loch) und die innere Fläche. Kreuzungen, T-Stöße und offene Ketten werden nicht
//! gemeldet.
//!
//! Abgelehnt werden entartete Kurven (Bogen ohne Winkel oder mit Ende neben seinem Kreis,
//! Ellipsenbogen mit Ende neben seiner Ellipse, zu kleine Ellipse, Spline mit zusammengefallenen
//! Nachbarpunkten; sie fallen aus der Anordnung) und Flächen, deren Rand sich selbst berührt
//! (Loch berührt den Rand in einem Punkt, innen tangentiale Kreise): Der Kernel kann solche
//! Schleifen nicht, Fusion schon (Abweichung, siehe `docs/superpowers/offene-punkte.md`).
//! „Berühren“ heißt wie im Kernel: näher als `tol.linear` plus die Sehnenabweichung der
//! Abtastung (`planora_kernel::SampledLoop`).
//!
//! Schleifen der Profile: Linie → `Segment::Line`, Kreis- und Bogenstück → `Segment::Arc`,
//! Ellipsenstück → `NurbsCurve::elliptic_arc`, Splinestück → Teil des natürlichen kubischen
//! Splines mit derselben Parametrisierung wie in der Skizze. Ein ungeteilter Kreis wird zu
//! [`ProfileLoop::circle`], eine ungeteilte Ellipse zum geschlossenen Freiformsegment ab
//! Parameter 0 (wie bisher). Jede Schleife beginnt beim Stück mit dem kleinsten
//! (Kurven-Index, `u0`) und durchläuft es in Kurvenrichtung.
//!
//! Die bisherige Extraktion bleibt als [`Sketch::legacy_profiles`] (`profile_legacy.rs`).

use crate::arrangement::{Arrangement, polygon_area, polygon_centroid, polygon_contains};
use crate::error::SketchError;
use crate::geom::Geom;
use crate::model::{CurveId, CurveKind, Sketch};
use crate::triangulate::triangulate;
use planora_i18n::tr;
use planora_kernel::{Profile, ProfileLoop, SampledLoop, Segment, segment_end};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, TAU, Tolerance};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Meldung für Bögen, deren Endpunkte nicht auf demselben Kreis um die Mitte liegen.
pub(crate) fn arc_off_circle() -> String {
    tr!("sketch-err-arc-off-circle")
}

/// Meldung für Ellipsenbögen, deren Endpunkte nicht auf ihrer Ellipse liegen.
pub(crate) fn elliptic_arc_off_ellipse() -> String {
    tr!("sketch-err-elliptic-arc-off-ellipse")
}

/// Meldung für Splines, deren Kurvenenden nicht an den Knoten der Schleife liegen.
pub(crate) fn spline_off_node() -> String {
    tr!("sketch-err-spline-off-node")
}

/// Teilungsstellen eines Splines, die relativ zum Parameterbereich höchstens so nah an einem
/// Stützpunkt liegen, werden auf ihn gelegt (deutlich über `MIN_KNOT_GAP_RATIO` = 1e-9).
const KNOT_SNAP: f64 = 1e-7;

/// Abtastschritte je Vollkreis für Schnitt- und Enthaltensein-Prüfungen (wie im Kernel).
pub(crate) const CHECK_STEPS: usize = 64;

/// Eine Schleife (oder Kurvengruppe), die nicht zu einem Profil werden kann, mit Grund.
#[derive(Clone, Debug, PartialEq)]
pub struct RejectedLoop {
    pub curves: Vec<CurveId>,
    pub reason: SketchError,
}

/// Stück einer Kurve am Rand eines Profils.
///
/// Normierter Parameter `u`: bei Kreis, Bogen, Ellipse und Ellipsenbogen der absolute Winkel
/// bzw. Ellipsenparameter (siehe [`Sketch::curve_eval`]) geteilt durch 2π, damit ein Kreis und
/// der später aus ihm gestutzte Bogen dieselbe Grundlage haben; bei Linie und Spline
/// `u = (t − lo)/(hi − lo)` über [`Sketch::curve_domain`]. Es gilt `u0 < u1`; bei Kreis und
/// Ellipse reicht `u1` bis `u0 + 1`.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceRef {
    pub curve: CurveId,
    pub u0: f64,
    pub u1: f64,
    /// Liegt die Fläche links der Kurvenrichtung?
    pub left: bool,
    /// Länge des Stücks in mm.
    pub len: f64,
}

/// Begrenzende Stücke eines Profils: äußerer Rand in Umlaufreihenfolge, danach die Stücke
/// aller Löcher.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileRef {
    pub outer: Vec<PieceRef>,
    pub holes: Vec<PieceRef>,
}

/// Ergebnis von [`Sketch::profiles`].
#[derive(Clone, Debug, Default)]
pub struct ExtractedProfiles {
    pub profiles: Vec<Profile>,
    /// Begrenzende Stücke von `profiles[i]`.
    pub refs: Vec<ProfileRef>,
    /// Abtastung von `profiles[i]` (64 Schritte je Vollkreis): `[0]` äußerer Umriss gegen den
    /// Uhrzeigersinn, danach die Löcher im Uhrzeigersinn; geschlossen gedacht.
    pub outlines: Vec<Vec<Vec<DVec2>>>,
    pub rejected: Vec<RejectedLoop>,
    /// Kurven des äußeren Rands von `profiles[i]` in Umlaufreihenfolge ohne direkte
    /// Wiederholung.
    pub curve_loops: Vec<Vec<CurveId>>,
    /// Die Anordnung, aus der die Profile stammen (für Stutzen und Profil-Verweise).
    pub(crate) arrangement: Option<Arc<Arrangement>>,
    /// Umläufe der Anordnung, die `profiles[i]` begrenzen (für [`ExtractedProfiles::merged`]).
    pub(crate) faces: Vec<FaceCycles>,
    /// Natürlicher kubischer Spline je Spline-Kurve der Anordnung (Index in
    /// `Arrangement::curves`).
    pub(crate) splines: Arc<HashMap<usize, Result<NurbsCurve, SketchError>>>,
    /// Toleranz der Skizze.
    pub(crate) tol: Tolerance,
    /// Dreiecke der Füllung je Profil, erst bei Bedarf berechnet ([`ExtractedProfiles::fill`]).
    pub(crate) fills: Vec<OnceLock<Vec<[DVec2; 3]>>>,
}

/// Umläufe der Anordnung, die ein Profil begrenzen: äußerer Umlauf und Löcher.
#[derive(Clone, Debug)]
pub(crate) struct FaceCycles {
    pub(crate) outer: usize,
    pub(crate) holes: Vec<usize>,
}

impl FaceCycles {
    /// Alle Umläufe (äußerer zuerst).
    pub(crate) fn cycles(&self) -> impl Iterator<Item = usize> + '_ {
        std::iter::once(self.outer).chain(self.holes.iter().copied())
    }
}

impl ExtractedProfiles {
    /// Index des Profils, das `p` enthält (bei mehreren das mit der kleinsten Fläche).
    pub fn profile_at(&self, p: DVec2) -> Option<usize> {
        self.outlines
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                o.first().is_some_and(|outer| polygon_contains(outer, p))
                    && !o[1..].iter().any(|h| polygon_contains(h, p))
            })
            .map(|(i, o)| (i, polygon_area(&o[0]).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Dreiecke, die die Fläche von Profil `i` füllen (aus `outlines[i]`, beim ersten Aufruf
    /// berechnet); leer für ein unbekanntes Profil.
    pub fn fill(&self, i: usize) -> &[[DVec2; 3]] {
        match (self.fills.get(i), self.outlines.get(i)) {
            (Some(cell), Some(outline)) => cell.get_or_init(|| triangulate(outline)),
            _ => &[],
        }
    }

    /// Legt die leeren Speicher der Füllungen an (eine je Profil).
    pub(crate) fn reset_fills(&mut self) {
        self.fills = (0..self.profiles.len()).map(|_| OnceLock::new()).collect();
    }
}

/// Fertig untersuchte Fläche der Anordnung.
struct Face {
    /// Sortierschlüssel: kleinster Kurven-Index des äußeren Rands, Schwerpunkt x, y.
    key: (usize, f64, f64),
    result: Result<Accepted, RejectedLoop>,
}

struct Accepted {
    profile: Profile,
    piece_ref: ProfileRef,
    outline: Vec<Vec<DVec2>>,
    curves: Vec<CurveId>,
    cycles: FaceCycles,
}

impl Sketch {
    /// Zerlegt die Skizze in Profile wie Fusion (siehe Modulbeschreibung) und meldet entartete
    /// Kurven sowie Flächen, die der Kernel nicht kann.
    ///
    /// Reihenfolge: Profile nach (kleinster Kurven-Id des äußeren Rands, Schwerpunkt x,
    /// Schwerpunkt y); abgelehnte Kurven in Id-Reihenfolge vor abgelehnten Flächen.
    pub fn profiles(&self) -> ExtractedProfiles {
        let tol = *self.tolerance();
        let mut out = ExtractedProfiles::default();
        let mut input: Vec<(CurveId, Geom)> = Vec::new();
        for (id, _) in self.curves().filter(|(_, c)| !c.construction) {
            match self.arrangement_geom(id) {
                Ok(Some(g)) => input.push((id, g)),
                Ok(None) => {}
                Err(reason) => out.rejected.push(RejectedLoop {
                    curves: vec![id],
                    reason,
                }),
            }
        }
        let arr = Arrangement::build(input, tol.linear);
        let min_area = tol.linear * tol.linear;

        // Löcher: Außenrand einer Komponente in der kleinsten umschließenden Fläche einer
        // anderen Komponente (Komponenten berühren sich nicht, ein Randpunkt entscheidet).
        let faces: Vec<usize> = (0..arr.cycles.len())
            .filter(|&c| arr.cycles[c].area > min_area)
            .collect();
        let mut holes: Vec<Vec<usize>> = vec![Vec::new(); arr.cycles.len()];
        for (ci, c) in arr.cycles.iter().enumerate() {
            let Some(&p) = c.polygon.first() else {
                continue;
            };
            if !(c.area.is_finite() && c.area < -min_area) {
                continue;
            }
            let host = faces
                .iter()
                .copied()
                .filter(|&f| {
                    let fc = &arr.cycles[f];
                    fc.component != c.component
                        && p.cmpge(fc.bbox.0).all()
                        && p.cmple(fc.bbox.1).all()
                        && polygon_contains(&fc.polygon, p)
                })
                .min_by(|&a, &b| arr.cycles[a].area.total_cmp(&arr.cycles[b].area));
            if let Some(f) = host {
                holes[f].push(ci);
            }
        }

        let splines: HashMap<usize, Result<NurbsCurve, SketchError>> = arr
            .curves
            .iter()
            .enumerate()
            .filter(|(_, (_, g))| matches!(g, Geom::Spline { .. }))
            .map(|(i, (id, _))| (i, self.natural_spline(*id)))
            .collect();
        let builder = LoopBuilder {
            tol,
            arr: &arr,
            splines: &splines,
        };
        let mut done: Vec<Face> = faces.iter().map(|&f| builder.face(f, &holes[f])).collect();
        done.sort_by(|x, y| {
            x.key
                .0
                .cmp(&y.key.0)
                .then(x.key.1.total_cmp(&y.key.1))
                .then(x.key.2.total_cmp(&y.key.2))
        });
        for face in done {
            match face.result {
                Ok(a) => {
                    out.profiles.push(a.profile);
                    out.refs.push(a.piece_ref);
                    out.outlines.push(a.outline);
                    out.curve_loops.push(a.curves);
                    out.faces.push(a.cycles);
                }
                Err(r) => out.rejected.push(r),
            }
        }
        out.arrangement = Some(Arc::new(arr));
        out.splines = Arc::new(splines);
        out.tol = tol;
        out.reset_fills();
        out
    }

    /// Natürlicher kubischer Spline durch die Stützpunkte des Splines `id` mit den Parametern
    /// `0, 1, …, n − 1` (wie [`Sketch::spline_point`]).
    fn natural_spline(&self, id: CurveId) -> Result<NurbsCurve, SketchError> {
        let CurveKind::Spline { points } = &self.curves[id].kind else {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-two-points")));
        };
        let pts: Vec<DVec3> = points
            .iter()
            .map(|&q| {
                let v = self.pos(q);
                DVec3::new(v.x, v.y, 0.0)
            })
            .collect();
        let params: Vec<f64> = (0..pts.len()).map(|i| i as f64).collect();
        NurbsCurve::natural_cubic(&pts, &params).map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-spline-invalid", detail = e.to_string()))
        })
    }

    /// Geometrie einer Kurve für die Anordnung; `Err` mit den bisherigen Texten für entartete
    /// Kurven, `Ok(None)` für Kurven, die still entfallen (Linie ohne Länge, nicht endlich).
    fn arrangement_geom(&self, id: CurveId) -> Result<Option<Geom>, SketchError> {
        let tol = self.tolerance();
        let no_angle = |phi: f64| phi <= tol.angular || phi >= TAU - tol.angular;
        match &self.curves[id].kind {
            CurveKind::Line { a, b } => {
                let len = self.pos(*a).distance(self.pos(*b));
                if !(len.is_finite() && len > tol.linear) {
                    return Ok(None);
                }
            }
            CurveKind::Circle { radius, .. } => {
                let r = self.vars[*radius];
                if !(r.is_finite() && r > tol.linear) {
                    return Err(SketchError::Degenerate(tr!("sketch-radius-positive")));
                }
            }
            &CurveKind::Arc { center, start, end } => {
                let c = self.pos(center);
                let (a, b) = (self.pos(start), self.pos(end));
                let (ra, rb) = ((a - c).length(), (b - c).length());
                if !(ra.is_finite() && ra > tol.linear) {
                    return Err(SketchError::Degenerate(tr!("sketch-radius-positive")));
                }
                if (ra - rb).abs() > tol.linear {
                    return Err(SketchError::Degenerate(arc_off_circle()));
                }
                let angle = |p: DVec2| (p - c).y.atan2((p - c).x);
                // wie bisher: (fast) kein oder ein voller Umlauf ist entartet
                if no_angle((angle(b) - angle(a)).rem_euclid(TAU)) {
                    return Err(SketchError::Degenerate(tr!("sketch-err-arc-no-angle")));
                }
            }
            CurveKind::Ellipse { .. } => {
                self.ellipse_frame(id)?;
            }
            &CurveKind::EllipticArc { start, end, .. } => {
                self.ellipse_frame(id)?;
                let (a, b) = (self.pos(start), self.pos(end));
                let (Some(t0), Some(t1)) =
                    (self.ellipse_param_of(id, a), self.ellipse_param_of(id, b))
                else {
                    return Err(SketchError::Degenerate(elliptic_arc_off_ellipse()));
                };
                if no_angle((t1 - t0).rem_euclid(TAU)) {
                    return Err(SketchError::Degenerate(tr!("sketch-err-arc-no-angle")));
                }
                let on = |t: f64, p: DVec2| {
                    self.ellipse_point(id, t)
                        .is_some_and(|q| q.distance(p) <= tol.linear)
                };
                if !(on(t0, a) && on(t1, b)) {
                    return Err(SketchError::Degenerate(elliptic_arc_off_ellipse()));
                }
            }
            CurveKind::Spline { points } => {
                if points.len() < 2 {
                    // Kann bei einer gültigen Skizze nicht auftreten (`add_spline` und das Laden
                    // verlangen zwei Punkte); zur Sicherheit benannt statt einer Panik.
                    return Err(SketchError::Degenerate(tr!("sketch-err-spline-two-points")));
                }
                // `add_spline` prüft das beim Anlegen; das Lösen kann Punkte danach zusammenlegen.
                if points
                    .windows(2)
                    .any(|w| self.pos(w[0]).distance(self.pos(w[1])) <= tol.linear)
                {
                    return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
                }
            }
        }
        let Some(g) = self.geom(id) else {
            return Ok(None);
        };
        let (lo, hi) = g.domain();
        let finite = g.eval(lo).0.is_finite() && g.eval(hi).0.is_finite() && g.bbox().0.is_finite();
        Ok(finite.then_some(g))
    }
}

/// Normierter Parameter `u` (siehe [`PieceRef`]).
pub(crate) fn normalized(g: &Geom, t: f64) -> f64 {
    match g {
        Geom::Line { .. } | Geom::Spline { .. } => {
            let (lo, hi) = g.domain();
            (t - lo) / (hi - lo)
        }
        _ => t / TAU,
    }
}

/// Teil `[t0, t1]` der Kurve `input` der Anordnung als Stück einer Schleife, vorwärts oder
/// rückwärts durchlaufen, von Knoten `from` nach Knoten `to`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Run {
    pub(crate) input: usize,
    pub(crate) t0: f64,
    pub(crate) t1: f64,
    pub(crate) forward: bool,
    pub(crate) from: usize,
    pub(crate) to: usize,
}

impl Run {
    /// Das Stück der Halbkante `h`.
    pub(crate) fn of(arr: &Arrangement, h: usize) -> Run {
        let p = &arr.pieces[h / 2];
        let (from, to) = arr.ends(h);
        Run {
            input: p.input,
            t0: p.t0,
            t1: p.t1,
            forward: h.is_multiple_of(2),
            from,
            to,
        }
    }
}

/// Baut Kernel-Schleifen aus Umläufen der Anordnung.
pub(crate) struct LoopBuilder<'a> {
    pub(crate) tol: Tolerance,
    pub(crate) arr: &'a Arrangement,
    /// Natürlicher kubischer Spline je Spline-Kurve der Anordnung.
    pub(crate) splines: &'a HashMap<usize, Result<NurbsCurve, SketchError>>,
}

impl LoopBuilder<'_> {
    /// Untersucht die Fläche `f` mit den Löchern `holes` (Umläufe der Anordnung).
    fn face(&self, f: usize, holes: &[usize]) -> Face {
        let arr = self.arr;
        let cycle = &arr.cycles[f];
        let (outer, reversed) = self.oriented(&cycle.half_edges);
        let mut curves: Vec<CurveId> = Vec::new();
        for &h in &outer {
            let c = arr.pieces[h / 2].curve;
            if curves.last() != Some(&c) {
                curves.push(c);
            }
        }
        if curves.len() > 1 && curves.first() == curves.last() {
            curves.pop();
        }
        let centroid = polygon_centroid(&cycle.polygon);
        let key = (
            curves.iter().map(|c| c.index()).min().unwrap_or(usize::MAX),
            centroid.x,
            centroid.y,
        );
        let reject = |reason: SketchError| RejectedLoop {
            curves: curves.clone(),
            reason,
        };
        // Löcher nach (kleinster Kurven-Index, Schwerpunkt)
        let mut holes: Vec<(usize, DVec2, usize, Vec<usize>, bool)> = holes
            .iter()
            .map(|&h| {
                let (hes, rev) = self.oriented(&arr.cycles[h].half_edges);
                let min = hes
                    .iter()
                    .map(|&e| arr.pieces[e / 2].curve.index())
                    .min()
                    .unwrap_or(usize::MAX);
                (min, polygon_centroid(&arr.cycles[h].polygon), h, hes, rev)
            })
            .collect();
        holes.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.x.total_cmp(&b.1.x))
                .then(a.1.y.total_cmp(&b.1.y))
        });

        let runs = |hes: &[usize]| -> Vec<Run> { hes.iter().map(|&h| Run::of(arr, h)).collect() };
        let result = (|| {
            let tol = &self.tol;
            let outer_lp = self.build(&runs(&outer))?;
            let mut hole_lps = Vec::with_capacity(holes.len());
            for (_, _, _, hes, _) in &holes {
                hole_lps.push(self.build(&runs(hes))?);
            }
            // Entscheidung 3: Ränder, die sich selbst oder einander berühren, kann der Kernel
            // nicht (eingeklemmte Flächen).
            let sampled: Vec<SampledLoop> = std::iter::once(&outer_lp)
                .chain(&hole_lps)
                .map(|lp| SampledLoop::new(lp, CHECK_STEPS, tol))
                .collect();
            let touching = (0..sampled.len()).any(|i| {
                sampled[i].touches_itself(tol.linear)
                    || (i + 1..sampled.len()).any(|j| sampled[i].touches(&sampled[j], tol.linear))
            });
            if touching {
                return Err(SketchError::SelfIntersecting);
            }
            Ok((outer_lp, hole_lps))
        })();
        let result = match result {
            Err(reason) => Err(reject(reason)),
            Ok((outer_lp, hole_lps)) => {
                let mut piece_ref = ProfileRef {
                    outer: self.piece_refs(&outer, reversed),
                    holes: Vec::new(),
                };
                let mut outline = vec![cycle.polygon.clone()];
                for (_, _, h, hes, rev) in &holes {
                    piece_ref.holes.extend(self.piece_refs(hes, *rev));
                    outline.push(arr.cycles[*h].polygon.clone());
                }
                Ok(Accepted {
                    profile: Profile {
                        outer: outer_lp,
                        holes: hole_lps,
                    },
                    piece_ref,
                    outline,
                    curves,
                    cycles: FaceCycles {
                        outer: f,
                        holes: holes.iter().map(|h| h.2).collect(),
                    },
                })
            }
        };
        Face { key, result }
    }

    /// Umlauf als Schleife: beginnt beim Stück mit kleinstem (Kurven-Index, `u0`) und
    /// durchläuft es vorwärts; `true`, wenn dafür der Umlauf umgekehrt wurde.
    fn oriented(&self, hes: &[usize]) -> (Vec<usize>, bool) {
        let arr = self.arr;
        let rank = |h: usize| {
            let p = &arr.pieces[h / 2];
            (p.curve.index(), normalized(arr.geom_of(h / 2), p.t0), h % 2)
        };
        let Some(first) = hes.iter().copied().min_by(|&x, &y| {
            let (a, b) = (rank(x), rank(y));
            a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2))
        }) else {
            return (Vec::new(), false);
        };
        let reversed = first % 2 == 1;
        let mut out: Vec<usize> = if reversed {
            hes.iter().rev().map(|&h| h ^ 1).collect()
        } else {
            hes.to_vec()
        };
        let want = if reversed { first ^ 1 } else { first };
        let at = out.iter().position(|&h| h == want).unwrap_or(0);
        out.rotate_left(at);
        (out, reversed)
    }

    /// Verweise der Stücke einer Schleife; `reversed`: die Schleife läuft gegen den Umlauf mit
    /// der Fläche links.
    fn piece_refs(&self, hes: &[usize], reversed: bool) -> Vec<PieceRef> {
        hes.iter()
            .map(|&h| {
                let p = &self.arr.pieces[h / 2];
                let g = self.arr.geom_of(h / 2);
                PieceRef {
                    curve: p.curve,
                    u0: normalized(g, p.t0),
                    u1: normalized(g, p.t1),
                    left: h.is_multiple_of(2) != reversed,
                    len: p.len,
                }
            })
            .collect()
    }

    /// Kernel-Schleife aus den Stücken `runs` (aneinander anschließend, das erste vorwärts).
    pub(crate) fn build(&self, runs: &[Run]) -> Result<ProfileLoop, SketchError> {
        let arr = self.arr;
        let tol = &self.tol;
        if let &[run] = runs
            && let Some(lp) = self.single(run)?
        {
            return Ok(lp);
        }
        let Some(first) = runs.first() else {
            return Err(SketchError::Degenerate(arc_off_circle()));
        };
        let start = arr.nodes[first.from];
        let mut at = start;
        let mut segments = Vec::with_capacity(runs.len());
        for &run in runs {
            let target = arr.nodes[run.to];
            let (seg, off_node) = match &arr.curves[run.input].1 {
                Geom::Line { .. } => (Segment::Line { to: target }, arc_off_circle()),
                Geom::Circle { c, .. } | Geom::Arc { c, .. } => (
                    Segment::Arc {
                        center: *c,
                        sweep: if run.forward {
                            run.t1 - run.t0
                        } else {
                            run.t0 - run.t1
                        },
                    },
                    arc_off_circle(),
                ),
                Geom::Ellipse { c, a, b } | Geom::EllipticArc { c, a, b, .. } => (
                    nurbs_segment(elliptic(*c, *a, *b, run.t0, run.t1 - run.t0)?, run.forward)?,
                    elliptic_arc_off_ellipse(),
                ),
                Geom::Spline { .. } => (
                    nurbs_segment(self.spline_piece(run)?, run.forward)?,
                    spline_off_node(),
                ),
            };
            if let Segment::Nurbs { curve } = &seg {
                // Der Kernel verlangt, dass ein Freiformsegment am Ende des vorigen beginnt.
                let q = curve.point(curve.domain().0);
                if at.distance(DVec2::new(q.x, q.y)) > tol.linear {
                    return Err(SketchError::Degenerate(off_node));
                }
            }
            at = segment_end(at, &seg);
            if at.distance(target) > tol.linear {
                return Err(SketchError::Degenerate(off_node));
            }
            segments.push(seg);
        }
        Ok(ProfileLoop { start, segments })
    }

    /// Schleife aus einem einzigen geschlossenen Stück: Kreis mit Knoten bei Winkel 0 wie
    /// [`ProfileLoop::circle`], Ellipse (bei Parameter 0 wie [`Sketch::ellipse_loop`]) und
    /// Splinestücke als geschlossenes Freiformsegment. `None` für die übrigen Stücke, die als
    /// gewöhnliche Segmente gebaut werden (ein Kreis, den ein einzelner Knoten an anderer Stelle
    /// teilt, etwa eine Berührung, wird so zu einem `Segment::Arc` mit 2π ab dem Knoten).
    fn single(&self, run: Run) -> Result<Option<ProfileLoop>, SketchError> {
        let kernel = |e: planora_kernel::KernelError| SketchError::Kernel(e.to_string());
        let lp = match &self.arr.curves[run.input].1 {
            Geom::Circle { c, r } if run.t0 == 0.0 => ProfileLoop::circle(*c, *r),
            Geom::Ellipse { c, a, b } => {
                ProfileLoop::nurbs_loop(elliptic(*c, *a, *b, run.t0, run.t1 - run.t0)?)
                    .map_err(kernel)?
            }
            Geom::Spline { .. } => {
                ProfileLoop::nurbs_loop(self.spline_piece(run)?).map_err(kernel)?
            }
            _ => return Ok(None),
        };
        Ok(Some(lp))
    }

    /// Teil `[t0, t1]` des natürlichen kubischen Splines zum Stück `run` (ganzer Spline, wenn
    /// das Stück ihn ganz überdeckt).
    fn spline_piece(&self, run: Run) -> Result<NurbsCurve, SketchError> {
        let invalid = |e: planora_math::nurbs::NurbsError| {
            SketchError::Degenerate(tr!("sketch-err-spline-invalid", detail = e.to_string()))
        };
        let whole = match self.splines.get(&run.input) {
            Some(Ok(curve)) => curve,
            Some(Err(e)) => return Err(e.clone()),
            None => return Err(SketchError::Degenerate(tr!("sketch-err-spline-two-points"))),
        };
        let (lo, hi) = whole.domain();
        if run.t0 <= lo && run.t1 >= hi {
            return Ok(whole.clone());
        }
        // Teilungsstellen dicht an einem Stützpunkt (Knoten bei ganzen `t`) auf ihn legen, sonst
        // entstünden zu nahe Knoten (Grenze `MIN_KNOT_GAP_RATIO` der NURBS).
        let snap = |t: f64| {
            let r = t.round();
            if (t - r).abs() <= KNOT_SNAP * (hi - lo) {
                r
            } else {
                t
            }
        };
        let (t0, t1) = (snap(run.t0.max(lo)), snap(run.t1.min(hi)));
        whole.sub_curve(t0, t1).map_err(invalid)
    }
}

/// Ellipsenstück `c + a·cos t + b·sin t` ab `t0` über `sweep` als Freiformkurve.
fn elliptic(c: DVec2, a: DVec2, b: DVec2, t0: f64, sweep: f64) -> Result<NurbsCurve, SketchError> {
    NurbsCurve::elliptic_arc(
        DVec3::new(c.x, c.y, 0.0),
        DVec3::new(a.x, a.y, 0.0),
        DVec3::new(b.x, b.y, 0.0),
        t0,
        sweep,
    )
    .map_err(|e| SketchError::Degenerate(tr!("sketch-err-ellipse-invalid", detail = e.to_string())))
}

/// Freiformsegment, bei `forward == false` umgekehrt (wie im Kernel).
fn nurbs_segment(curve: NurbsCurve, forward: bool) -> Result<Segment, SketchError> {
    let start = curve.point(curve.domain().0);
    let seg = Segment::Nurbs {
        curve: Arc::new(curve),
    };
    if forward {
        return Ok(seg);
    }
    let single = ProfileLoop {
        start: DVec2::new(start.x, start.y),
        segments: vec![seg],
    };
    let reversed = single
        .try_reversed()
        .map_err(|e| SketchError::Kernel(e.to_string()))?;
    reversed
        .segments
        .into_iter()
        .next()
        .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-spline-reverse")))
}

impl Sketch {
    /// Schleife aus einer Ellipse der Skizze: ein geschlossenes Freiformsegment
    /// `c + a·cos t + b·sin t` mit `b = minor · (−ay, ax)/|a|` (gegen den Uhrzeigersinn, wie
    /// [`Sketch::ellipse_point`]).
    pub(crate) fn ellipse_loop(&self, id: CurveId) -> Result<ProfileLoop, SketchError> {
        let (c, a, b) = self.ellipse_frame(id)?;
        let curve = NurbsCurve::elliptic_arc(
            DVec3::new(c.x, c.y, 0.0),
            DVec3::new(a.x, a.y, 0.0),
            DVec3::new(b.x, b.y, 0.0),
            0.0,
            TAU,
        )
        .map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-ellipse-invalid", detail = e.to_string()))
        })?;
        ProfileLoop::nurbs_loop(curve).map_err(|e| SketchError::Kernel(e.to_string()))
    }

    /// Mittelpunkt `c` und Halbachsen `a`, `b = minor · (−ay, ax)/|a|` einer Ellipse oder eines
    /// Ellipsenbogens; beide Halbachsen müssen länger als `tol.linear` sein.
    pub(crate) fn ellipse_frame(&self, id: CurveId) -> Result<(DVec2, DVec2, DVec2), SketchError> {
        let (c, a, minor) = self
            .ellipse_axes(id)
            .ok_or_else(|| SketchError::WrongType(tr!("sketch-err-not-ellipse")))?;
        let tol = self.tolerance().linear;
        let len = a.length();
        if !(len.is_finite() && len > tol) {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-ellipse-major-small",
                tol = planora_i18n::len_arg(tol, tol.to_string()),
                unit = planora_i18n::unit_symbol()
            )));
        }
        if !(minor.is_finite() && minor > tol) {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-ellipse-minor-small",
                tol = planora_i18n::len_arg(tol, tol.to_string()),
                unit = planora_i18n::unit_symbol()
            )));
        }
        Ok((c, a, a.perp() / len * minor))
    }

    /// Freiformsegment eines Ellipsenbogens von Knoten `from` (Start) gegen den Uhrzeigersinn zu
    /// Knoten `to` (Ende) auf seiner Ellipse: `NurbsCurve::elliptic_arc` mit dem Parameter des
    /// Starts als Startwinkel und dem Parameterabstand zum Ende als Öffnungswinkel; umgekehrt,
    /// wenn die Schleife ihn gegen seine Richtung durchläuft. Liegen die Knoten nicht auf der
    /// Ellipse, beginnt oder endet das Segment neben ihnen (das prüft der Aufrufer).
    pub(crate) fn elliptic_arc_segment(
        &self,
        id: CurveId,
        from: DVec2,
        to: DVec2,
        forward: bool,
    ) -> Result<Segment, SketchError> {
        let tol = self.tolerance();
        let (c, a, b) = self.ellipse_frame(id)?;
        let (Some(t0), Some(t1)) = (
            self.ellipse_param_of(id, from),
            self.ellipse_param_of(id, to),
        ) else {
            return Err(SketchError::Degenerate(elliptic_arc_off_ellipse()));
        };
        let sweep = (t1 - t0).rem_euclid(TAU);
        // wie beim Bogen: (fast) kein oder ein voller Umlauf zwischen zwei Knoten ist entartet
        if sweep <= tol.angular || sweep >= TAU - tol.angular {
            return Err(SketchError::Degenerate(tr!("sketch-err-arc-no-angle")));
        }
        let curve = NurbsCurve::elliptic_arc(
            DVec3::new(c.x, c.y, 0.0),
            DVec3::new(a.x, a.y, 0.0),
            DVec3::new(b.x, b.y, 0.0),
            t0,
            sweep,
        )
        .map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-ellipse-invalid", detail = e.to_string()))
        })?;
        let start = curve.point(curve.domain().0);
        let seg = Segment::Nurbs {
            curve: Arc::new(curve),
        };
        if forward {
            return Ok(seg);
        }
        let single = ProfileLoop {
            start: DVec2::new(start.x, start.y),
            segments: vec![seg],
        };
        let reversed = single
            .try_reversed()
            .map_err(|e| SketchError::Kernel(e.to_string()))?;
        reversed
            .segments
            .into_iter()
            .next()
            .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-elliptic-arc-reverse")))
    }

    /// Freiformsegment eines Splines: natürlicher kubischer Spline durch die Stützpunkte mit
    /// den Parametern `0, 1, …, n − 1` (wie [`Sketch::spline_point`]); umgekehrt, wenn die
    /// Schleife ihn gegen seine Richtung durchläuft.
    pub(crate) fn spline_segment(
        &self,
        points: &[crate::model::PointId],
        forward: bool,
    ) -> Result<Segment, SketchError> {
        let tol = self.tolerance().linear;
        let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
        // `add_spline` prüft das beim Anlegen; das Lösen kann Punkte danach zusammenlegen.
        if xy.windows(2).any(|w| w[0].distance(w[1]) <= tol) {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
        }
        let pts: Vec<DVec3> = xy.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect();
        let params: Vec<f64> = (0..pts.len()).map(|i| i as f64).collect();
        let curve = NurbsCurve::natural_cubic(&pts, &params).map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-spline-invalid", detail = e.to_string()))
        })?;
        let seg = Segment::Nurbs {
            curve: Arc::new(curve),
        };
        if forward {
            return Ok(seg);
        }
        // Umkehr wie im Kernel (gespiegelte Knoten, bei Rundungsproblemen negierte Knoten).
        let single = ProfileLoop {
            start: xy[0],
            segments: vec![seg],
        };
        let reversed = single
            .try_reversed()
            .map_err(|e| SketchError::Kernel(e.to_string()))?;
        reversed
            .segments
            .into_iter()
            .next()
            .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-spline-reverse")))
    }
}
