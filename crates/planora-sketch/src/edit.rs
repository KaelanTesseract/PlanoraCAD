//! Ändern der Skizze wie in Fusion: Stutzen (hier), Dehnen und Brechen.
//!
//! Stutzen löscht das Stück einer Kurve zwischen den Schnittpunkten links und rechts der
//! Klickstelle (mit allen Kurven, auch Konstruktion; bei Kreis und Ellipse umlaufend); hat die
//! Kurve keinen Schnitt, wird sie ganz gelöscht. Abhängigkeiten nach Entscheidung 7 des Plans:
//! Erhalten bleibt, was danach ohne Bewegung erfüllt ist, so bewegt sich nie etwas; neue Enden
//! hängen „deckungsgleich“ an einem Punkt der schneidenden Kurve oder „Punkt auf Kurve“ an ihr.

use crate::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use crate::error::SketchError;
use crate::geom::Geom;
use crate::intersect::clean_params;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_i18n::tr;
use planora_math::{DVec2, TAU};
use std::collections::HashSet;

/// Abschnitte je Vollkreis des Linienzugs der Vorschau (wie `curve_polyline` in der Ansicht).
const PREVIEW_STEPS_PER_TURN: usize = 64;
/// Vielfaches von `tol.linear` bzw. `tol.angular`, bis zu dem eine Abhängigkeit nach dem
/// Ändern als erfüllt gilt (Entscheidung 7).
const SETTLE_FACTOR: f64 = 10.0;

/// Das Stück, das [`Sketch::trim`] an derselben Stelle löschen würde.
#[derive(Clone, Debug, PartialEq)]
pub struct TrimPiece {
    pub curve: CurveId,
    /// Parameterbereich des Stücks (siehe [`Sketch::curve_eval`]); bei Kreis und Ellipse kann
    /// `t0` negativ oder `t1` größer als 2π sein (umlaufend), es gilt immer `t0 < t1`.
    pub t0: f64,
    pub t1: f64,
    /// Die ganze Kurve (kein Schnitt, bei Kreis und Ellipse auch genau einer).
    pub whole: bool,
    /// Linienzug des Stücks von `t0` nach `t1` zum Hervorheben.
    pub polyline: Vec<DVec2>,
}

/// Grenzen des Stücks mit der schneidenden Kurve an jeder Grenze (`None` am Kurvenende, bei
/// einem Selbstschnitt und beim ganzen Stück).
struct Bounds {
    t0: f64,
    t1: f64,
    whole: bool,
    /// Es gibt eine Schnittstelle bei `t0` bzw. `t1` (sonst Kurvenende).
    cut0: bool,
    cut1: bool,
    cutter0: Option<CurveId>,
    cutter1: Option<CurveId>,
}

/// Neues Ende: Punkt, schneidende Kurve und Schnittstelle.
type NewEnd = (PointId, Option<CurveId>, DVec2);

/// Abstand zweier Parameter, bei periodischen Kurven über den kürzeren Weg.
fn param_gap(g: &Geom, a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    if g.periodic() {
        let d = d.rem_euclid(TAU);
        d.min(TAU - d)
    } else {
        d
    }
}

impl Sketch {
    /// Das Stück der Kurve `c`, das ein Klick bei `at` stutzen würde (Vorschau beim
    /// Überfahren). `None` für unbekannte oder entartete Kurven und nicht endliches `at`.
    pub fn trim_piece(&self, c: CurveId, at: DVec2) -> Option<TrimPiece> {
        let (g, b) = self.locate_trim(c, at)?;
        Some(TrimPiece {
            curve: c,
            t0: b.t0,
            t1: b.t1,
            whole: b.whole,
            polyline: g.polyline(b.t0, b.t1, PREVIEW_STEPS_PER_TURN),
        })
    }

    /// Stutzt die Kurve `c` an der Stelle `at` (siehe [`Sketch::trim_piece`]): Linie, Bogen,
    /// Ellipsenbogen und Spline werden kürzer oder zerfallen in zwei; ein Kreis wird ein Bogen,
    /// eine Ellipse ein Ellipsenbogen (beide behalten ihre Id und ihre Maße). Ohne Schnitt wird
    /// die Kurve gelöscht, mit den Punkten, die nur sie benutzte. Gibt die übrigen Kurven
    /// zurück, `c` zuerst, falls sie bleibt. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn trim(&mut self, c: CurveId, at: DVec2) -> Result<Vec<CurveId>, SketchError> {
        self.check_curve(c)?;
        if !at.is_finite() {
            return Err(SketchError::InvalidInput(tr!("sketch-err-target-finite")));
        }
        let Some((g, b)) = self.locate_trim(c, at) else {
            return Err(SketchError::Degenerate(tr!("sketch-err-trim-degenerate")));
        };
        let backup = self.clone();
        let result = self.apply_trim(c, &g, &b);
        if result.is_err() {
            *self = backup;
        }
        result
    }

    /// Geometrie der Kurve und Grenzen des Stücks um den Kurvenpunkt, der `at` am nächsten ist.
    fn locate_trim(&self, c: CurveId, at: DVec2) -> Option<(Geom, Bounds)> {
        if !at.is_finite() {
            return None;
        }
        let g = self.geom(c)?;
        let raw = self.cuts(c, &g, true);
        let cleaned = clean_params(
            &g,
            raw.iter().map(|r| r.0).collect(),
            self.tolerance().linear,
        );
        let (lo, hi) = g.domain();
        let here = g.nearest(at);
        let whole = |t0: f64, t1: f64| Bounds {
            t0,
            t1,
            whole: true,
            cut0: false,
            cut1: false,
            cutter0: None,
            cutter1: None,
        };
        if cleaned.is_empty() {
            return Some((g, whole(lo, hi)));
        }
        if g.periodic() && cleaned.len() == 1 {
            return Some((g, whole(cleaned[0], cleaned[0] + TAU)));
        }
        // Index der Schnittstelle links (≤) und rechts (>) von `here`
        let left = cleaned.iter().rposition(|&t| t <= here);
        let right = cleaned.iter().position(|&t| t > here);
        let last = cleaned.len() - 1;
        let (k0, k1, t0, t1) = if g.periodic() {
            let k0 = left.unwrap_or(last);
            let k1 = right.unwrap_or(0);
            let t0 = if left.is_some() {
                cleaned[k0]
            } else {
                cleaned[k0] - TAU
            };
            let t1 = if right.is_some() {
                cleaned[k1]
            } else {
                cleaned[k1] + TAU
            };
            (Some(k0), Some(k1), t0, t1)
        } else {
            (
                left,
                right,
                left.map_or(lo, |k| cleaned[k]),
                right.map_or(hi, |k| cleaned[k]),
            )
        };
        // Jeder rohe Schnitt gehört zur nächsten bereinigten Stelle (bei offenen Kurven zählen
        // die Enden mit: dort weggefallene Schnitte gehören zu keiner Stelle).
        let cutter = |k: Option<usize>| -> Option<CurveId> {
            let k = k?;
            raw.iter()
                .filter_map(|&(t, id)| {
                    let t = g.normalize(t);
                    let gap = |u: f64| param_gap(&g, t, u);
                    let nearest = (0..cleaned.len())
                        .min_by(|&x, &y| gap(cleaned[x]).total_cmp(&gap(cleaned[y])))?;
                    let to_end = if g.periodic() {
                        f64::INFINITY
                    } else {
                        gap(lo).min(gap(hi))
                    };
                    (nearest == k && gap(cleaned[k]) <= to_end).then_some(id)?
                })
                .min()
        };
        let b = Bounds {
            t0,
            t1,
            whole: false,
            cut0: k0.is_some(),
            cut1: k1.is_some(),
            cutter0: cutter(k0),
            cutter1: cutter(k1),
        };
        Some((g, b))
    }

    fn apply_trim(
        &mut self,
        c: CurveId,
        g: &Geom,
        b: &Bounds,
    ) -> Result<Vec<CurveId>, SketchError> {
        let kind = self.curves[c].kind.clone();
        let old_points = kind.points();
        if b.whole {
            self.remove_curve(c)?;
            self.drop_orphans(&old_points)?;
            return Ok(Vec::new());
        }
        let mut ends: Vec<NewEnd> = Vec::new();
        let mut out = vec![c];
        match kind {
            CurveKind::Circle { center, .. } => {
                let start = self.cut_point(g, b.t1, b.cutter1, &mut ends)?;
                let end = self.cut_point(g, b.t0, b.cutter0, &mut ends)?;
                self.set_curve_kind(c, CurveKind::Arc { center, start, end });
                self.push_internal(ConstraintKind::ArcRadius(c));
            }
            CurveKind::Ellipse {
                center,
                ax,
                ay,
                minor,
            } => {
                let start = self.cut_point(g, b.t1, b.cutter1, &mut ends)?;
                let end = self.cut_point(g, b.t0, b.cutter0, &mut ends)?;
                self.set_curve_kind(
                    c,
                    CurveKind::EllipticArc {
                        center,
                        ax,
                        ay,
                        minor,
                        start,
                        end,
                    },
                );
                self.push_internal(ConstraintKind::EllipticArcEnds(c));
            }
            open => {
                let (lo, hi) = g.domain();
                let (first, last) = match &open {
                    CurveKind::Line { a, b } => (*a, *b),
                    CurveKind::Arc { start, end, .. }
                    | CurveKind::EllipticArc { start, end, .. } => (*start, *end),
                    CurveKind::Spline { points } => (points[0], points[points.len() - 1]),
                    CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => {
                        unreachable!("closed curves are handled above")
                    }
                };
                let mut pieces = Vec::new();
                if b.cut0 {
                    let e = self.cut_point(g, b.t0, b.cutter0, &mut ends)?;
                    pieces.push((first, e, lo, b.t0));
                }
                if b.cut1 {
                    let s = self.cut_point(g, b.t1, b.cutter1, &mut ends)?;
                    pieces.push((s, last, b.t1, hi));
                }
                for (k, (s, e, u0, u1)) in pieces.into_iter().enumerate() {
                    let piece = match &open {
                        CurveKind::Line { .. } => CurveKind::Line { a: s, b: e },
                        CurveKind::Arc { center, .. } => CurveKind::Arc {
                            center: *center,
                            start: s,
                            end: e,
                        },
                        &CurveKind::EllipticArc {
                            center,
                            ax,
                            ay,
                            minor,
                            ..
                        } => CurveKind::EllipticArc {
                            center,
                            ax,
                            ay,
                            minor,
                            start: s,
                            end: e,
                        },
                        CurveKind::Spline { points } => CurveKind::Spline {
                            points: self.spline_piece(g, points, s, e, u0, u1)?,
                        },
                        CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => {
                            unreachable!("closed curves are handled above")
                        }
                    };
                    if k == 0 {
                        self.set_curve_kind(c, piece);
                    } else {
                        out.push(self.add_piece(piece)?);
                    }
                }
            }
        }
        self.drop_orphans(&old_points)?;
        self.settle_constraints(&[c], &[]);
        if let Some(&n) = out.get(1) {
            self.link_pieces(c, n)?;
        }
        for (p, cutter, at) in ends {
            if let Some(k) = cutter {
                self.end_constraint(p, k, at);
            }
        }
        Ok(out)
    }

    /// Neuer Punkt der Kurve beim Parameter `t`, vermerkt als neues Ende. Liegt dort ein Punkt
    /// der schneidenden Kurve, kommt der neue Punkt genau auf ihn (für „deckungsgleich“).
    fn cut_point(
        &mut self,
        g: &Geom,
        t: f64,
        cutter: Option<CurveId>,
        ends: &mut Vec<NewEnd>,
    ) -> Result<PointId, SketchError> {
        let mut at = g.eval(t).0;
        if let Some(q) = cutter.and_then(|k| self.cutter_point_at(k, at)) {
            at = self.pos(q);
        }
        let p = self.add_point(at.x, at.y)?;
        ends.push((p, cutter, at));
        Ok(p)
    }

    /// Der Punkt der Kurve `k`, der höchstens `tol.linear` von `at` entfernt liegt (der nächste).
    fn cutter_point_at(&self, k: CurveId, at: DVec2) -> Option<PointId> {
        let tol = self.tolerance().linear;
        self.curves[k]
            .kind
            .points()
            .into_iter()
            .filter(|&q| self.check_point(q).is_ok())
            .map(|q| (q, self.pos(q).distance(at)))
            .filter(|&(_, d)| d <= tol)
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(q, _)| q)
    }

    /// Stützpunkte des Spline-Stücks `u0..u1` von `s` nach `e` (Entscheidung 6): die
    /// Stützpunkte dazwischen (ohne die, die auf einem Ende liegen); gibt es keinen, der
    /// Kurvenpunkt in der Bereichsmitte als neuer Stützpunkt.
    fn spline_piece(
        &mut self,
        g: &Geom,
        points: &[PointId],
        s: PointId,
        e: PointId,
        u0: f64,
        u1: f64,
    ) -> Result<Vec<PointId>, SketchError> {
        let tol = self.tolerance().linear;
        let (ps, pe) = (self.pos(s), self.pos(e));
        let mut out = vec![s];
        for (i, &p) in points.iter().enumerate() {
            let t = i as f64;
            let q = self.pos(p);
            if t > u0 && t < u1 && q.distance(ps) > tol && q.distance(pe) > tol {
                out.push(p);
            }
        }
        if out.len() == 1 {
            let m = g.eval(0.5 * (u0 + u1)).0;
            out.push(self.add_point(m.x, m.y)?);
        }
        out.push(e);
        if out
            .windows(2)
            .any(|w| self.pos(w[0]).distance(self.pos(w[1])) <= tol)
        {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
        }
        Ok(out)
    }

    /// Legt das zweite Teilstück als neue Kurve an (mit den Prüfungen der `add_…`-Funktionen;
    /// ein Ellipsenbogen teilt die Variablen seiner Ellipse).
    fn add_piece(&mut self, piece: CurveKind) -> Result<CurveId, SketchError> {
        match piece {
            CurveKind::Line { a, b } => self.add_line(a, b),
            CurveKind::Arc { center, start, end } => self.add_arc(center, start, end),
            CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                start,
                end,
            } => Ok(self.add_elliptic_arc_vars(center, ax, ay, minor, start, end)),
            CurveKind::Spline { points } => self.add_spline(&points),
            CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => {
                unreachable!("pieces are open curves")
            }
        }
    }

    /// Abhängigkeiten zwischen Original `c` und neuem Teilstück `n` (Entscheidung 7): Linie
    /// waagerecht/senkrecht wie das Original, sonst parallel, und ihr Anfang auf dem Original
    /// (zusammen kollinear); Bogen gleich groß; Ellipsenbogen teilt ohnehin die Variablen.
    fn link_pieces(&mut self, c: CurveId, n: CurveId) -> Result<(), SketchError> {
        match self.curves[n].kind {
            CurveKind::Line { a, .. } => {
                let line = LineOrPoints::Line(c);
                let active = |k: &ConstraintKind| {
                    self.constraints()
                        .any(|(_, d)| d.enabled && !d.driven && d.kind == *k)
                };
                let direction = if active(&ConstraintKind::Horizontal(line.clone())) {
                    ConstraintKind::Horizontal(LineOrPoints::Line(n))
                } else if active(&ConstraintKind::Vertical(line)) {
                    ConstraintKind::Vertical(LineOrPoints::Line(n))
                } else {
                    ConstraintKind::Parallel(c, n)
                };
                self.add_constraint(direction)?;
                self.add_constraint(ConstraintKind::PointOnCurve(a, c))?;
            }
            CurveKind::Arc { .. } => {
                self.add_constraint(ConstraintKind::Equal(c, n))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Entfernt die Punkte aus `points`, die keine lebende Kurve mehr benutzt (samt ihren
    /// Abhängigkeiten).
    fn drop_orphans(&mut self, points: &[PointId]) -> Result<(), SketchError> {
        let used: HashSet<PointId> = self.curves().flat_map(|(_, d)| d.kind.points()).collect();
        for &p in points {
            if !used.contains(&p) && self.check_point(p).is_ok() {
                self.remove_point(p)?;
            }
        }
        Ok(())
    }

    /// Regel 7 nach einer Änderung an den Kurven `touched` und den Punkten `points`: Die
    /// Zusatzvariablen von „Punkt auf Kurve“ und „tangential“ an (neu gelegten) Splines
    /// werden neu gesetzt; jede aktive Abhängigkeit an ihnen, die nicht mehr ohne Bewegung
    /// erfüllt ist, entfällt, ebenso „Mittelpunkt“ an einer geänderten Linie. Nur anzeigende
    /// und ausgeschaltete Abhängigkeiten bewegen nichts und bleiben.
    pub(crate) fn settle_constraints(&mut self, touched: &[CurveId], points: &[PointId]) {
        let ids: Vec<ConstraintId> = self
            .constraints()
            .filter(|(_, d)| {
                d.kind.curves().iter().any(|c| touched.contains(c))
                    || d.kind.points().iter().any(|p| points.contains(p))
            })
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            let kind = self.constraints[id].kind.clone();
            if let ConstraintKind::Midpoint(_, l) = kind
                && touched.contains(&l)
            {
                self.drop_constraint(id);
                continue;
            }
            let on_touched_spline = kind.curves().iter().any(|c| {
                touched.contains(c) && matches!(self.curves[*c].kind, CurveKind::Spline { .. })
            });
            if on_touched_spline
                && self.needs_extra(&kind)
                && let (Some(var), Some(t)) = (self.constraints[id].extra, self.extra_start(&kind))
            {
                self.vars[var] = t;
                self.touch_values();
            }
            let d = &self.constraints[id];
            if d.enabled && !d.driven && !self.satisfied(id, SETTLE_FACTOR) {
                self.drop_constraint(id);
            }
        }
    }

    fn drop_constraint(&mut self, id: ConstraintId) {
        self.constraints[id].alive = false;
        self.touch_structure();
    }

    /// Abhängigkeit des neuen Endes `p` an der schneidenden Kurve `cutter` (Schnitt bei `at`):
    /// „deckungsgleich“ mit einem Punkt der Kurve, der höchstens `tol.linear` entfernt liegt
    /// (`p` kommt genau auf ihn), sonst „Punkt auf Kurve“.
    pub(crate) fn end_constraint(&mut self, p: PointId, cutter: CurveId, at: DVec2) {
        let kind = match self.cutter_point_at(cutter, at).filter(|&q| q != p) {
            Some(q) => {
                let v = self.pos(q);
                let (x, y) = (self.points[p].x, self.points[p].y);
                if (self.vars[x], self.vars[y]) != (v.x, v.y) {
                    self.vars[x] = v.x;
                    self.vars[y] = v.y;
                    self.touch_values();
                }
                ConstraintKind::Coincident(p, q)
            }
            None => ConstraintKind::PointOnCurve(p, cutter),
        };
        // Beide Arten passen zu jeder Kurve; ein Fehler kann nur bei ungültigen Ids entstehen.
        let _ = self.add_constraint(kind);
    }
}
