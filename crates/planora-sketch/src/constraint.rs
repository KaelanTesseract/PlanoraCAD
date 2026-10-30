//! Abhängigkeiten und Maße. Jede Abhängigkeit liefert Residuen als `Dual`, also mit exakten
//! Ableitungen nach den beteiligten Variablen.

use crate::curves::{Sampled, ellipse_eval, spline_eval, spline_second_derivatives};
use crate::dual::Dual;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_i18n::tr;
use planora_kernel::Id;
use planora_math::DVec2;
use std::cell::RefCell;

/// Mindestlänge für Richtungsvektoren beim Normieren (verhindert Division durch null).
const MIN_LEN: f64 = 1e-9;
/// Untergrenze des Spielraums (in Parametereinheiten), mit dem ein Spline-Parameter über
/// `[0, n − 1]` hinausragen darf (sehr lange End-Segmente).
const MIN_SPLINE_T_TOL: f64 = 1e-12;
/// Obergrenze dieses Spielraums (zu einem Punkt zusammengezogene End-Segmente).
const MAX_SPLINE_T_TOL: f64 = 1e-3;

/// Bereich des Spline-Parameters einer parametrischen Abhängigkeit: Variable, Ende `n − 1` und
/// erlaubter Überstand an beiden Enden.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SplineParameter {
    pub var: usize,
    pub end: f64,
    pub slack_start: f64,
    pub slack_end: f64,
}

impl SplineParameter {
    /// Liegt `t` im (um den Spielraum erweiterten) Bereich? NaN liegt nie darin.
    pub fn contains(&self, t: f64) -> bool {
        t >= -self.slack_start && t <= self.end + self.slack_end
    }
}

/// Eine Linie oder zwei Punkte (für waagerecht/senkrecht).
#[derive(Clone, Debug, PartialEq)]
pub enum LineOrPoints {
    Line(CurveId),
    Points(PointId, PointId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstraintKind {
    Coincident(PointId, PointId),
    PointOnCurve(PointId, CurveId),
    Horizontal(LineOrPoints),
    Vertical(LineOrPoints),
    Parallel(CurveId, CurveId),
    Perpendicular(CurveId, CurveId),
    Tangent(CurveId, CurveId),
    Equal(CurveId, CurveId),
    Fixed(PointId, f64, f64),
    /// Zwei Punkte symmetrisch zur Achse (einer Linie).
    Symmetric(PointId, PointId, CurveId),
    Midpoint(PointId, CurveId),
    Distance(PointId, PointId, f64),
    DistancePointLine(PointId, CurveId, f64),
    Radius(CurveId, f64),
    Diameter(CurveId, f64),
    /// Winkel von Richtung 1 nach Richtung 2 in Radiant.
    Angle(CurveId, CurveId, f64),
    /// Intern: das Ende eines Bogens liegt auf dem Kreis durch seinen Start.
    ArcRadius(CurveId),
    /// Länge der großen Halbachse einer Ellipse. Gibt es auf derselben Ellipse ein erzwungenes
    /// `EllipseMinor`, darf der Wert nicht kleiner als dessen Wert sein.
    EllipseMajor(CurveId, f64),
    /// Länge der kleinen Halbachse einer Ellipse. Gibt es auf derselben Ellipse ein erzwungenes
    /// `EllipseMajor`, darf der Wert nicht größer als dessen Wert sein.
    EllipseMinor(CurveId, f64),
    /// Winkel der großen Halbachse einer Ellipse zur x-Achse der Skizze in Radiant.
    ///
    /// Grenze: Das Residuum `cross(a, (cos v, sin v))/|a|` ist auch null, wenn die große
    /// Halbachse in die Gegenrichtung `v + π` zeigt. Das ist dieselbe Ellipse; `measure`
    /// liefert dann aber `v ± π` (Wert von `atan2` in `(−π, π]`).
    EllipseAngle(CurveId, f64),
}

impl ConstraintKind {
    /// Ist das ein Maß (mit Zahlenwert)?
    pub fn is_dimension(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Distance(..)
                | ConstraintKind::DistancePointLine(..)
                | ConstraintKind::Radius(..)
                | ConstraintKind::Diameter(..)
                | ConstraintKind::Angle(..)
                | ConstraintKind::EllipseMajor(..)
                | ConstraintKind::EllipseMinor(..)
                | ConstraintKind::EllipseAngle(..)
        )
    }

    /// Dieselbe Abhängigkeit mit dem Zahlenwert `v`; `None`, wenn sie kein Maß ist. Prüft
    /// nichts (dafür gibt es [`Sketch::check_constraint_value`]).
    pub fn with_value(&self, v: f64) -> Option<ConstraintKind> {
        use ConstraintKind::*;
        let mut kind = self.clone();
        match &mut kind {
            Distance(_, _, x)
            | DistancePointLine(_, _, x)
            | Angle(_, _, x)
            | Radius(_, x)
            | Diameter(_, x)
            | EllipseMajor(_, x)
            | EllipseMinor(_, x)
            | EllipseAngle(_, x) => *x = v,
            _ => return None,
        }
        Some(kind)
    }

    /// Die direkt beteiligten Punkte.
    pub fn points(&self) -> Vec<PointId> {
        use ConstraintKind::*;
        match self {
            Coincident(a, b) | Distance(a, b, _) | Symmetric(a, b, _) => vec![*a, *b],
            Horizontal(LineOrPoints::Points(a, b)) | Vertical(LineOrPoints::Points(a, b)) => {
                vec![*a, *b]
            }
            PointOnCurve(p, _) | Fixed(p, ..) | Midpoint(p, _) | DistancePointLine(p, ..) => {
                vec![*p]
            }
            _ => vec![],
        }
    }

    /// Die direkt beteiligten Kurven.
    pub fn curves(&self) -> Vec<CurveId> {
        use ConstraintKind::*;
        match self {
            PointOnCurve(_, c)
            | Midpoint(_, c)
            | DistancePointLine(_, c, _)
            | Radius(c, _)
            | Diameter(c, _)
            | ArcRadius(c)
            | EllipseMajor(c, _)
            | EllipseMinor(c, _)
            | EllipseAngle(c, _)
            | Symmetric(_, _, c)
            | Horizontal(LineOrPoints::Line(c))
            | Vertical(LineOrPoints::Line(c)) => vec![*c],
            Parallel(a, b) | Perpendicular(a, b) | Tangent(a, b) | Equal(a, b) | Angle(a, b, _) => {
                vec![*a, *b]
            }
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConstraintData {
    pub kind: ConstraintKind,
    /// Gewählte Lage bei mehrdeutigen Abhängigkeiten (Tangente, Abstand Punkt–Linie).
    pub(crate) branch: i8,
    /// Nur anzeigen, nicht erzwingen.
    pub driven: bool,
    pub enabled: bool,
    pub internal: bool,
    /// Zusatzvariable (Index im Variablenvektor) parametrischer Abhängigkeiten: der
    /// Kurvenparameter `t` bei Punkt auf Spline und Tangente Linie–Ellipse/Spline. Sie wird mit
    /// der Abhängigkeit angelegt; entfällt die Abhängigkeit, bleibt sie unbenutzt im Vektor.
    pub(crate) extra: Option<usize>,
    pub(crate) alive: bool,
}
pub type ConstraintId = Id<ConstraintData>;

impl ConstraintData {
    /// Gewählte Lage bei mehrdeutigen Abhängigkeiten: `1`/`-1` je Seite, `0` ohne Wahl.
    pub fn branch(&self) -> i8 {
        self.branch
    }

    /// Index der Zusatzvariablen (Kurvenparameter) parametrischer Abhängigkeiten, sonst `None`.
    pub fn extra(&self) -> Option<usize> {
        self.extra
    }
}

/// Ergebnis einer Auswertung: beteiligte Variablen und Residuen mit Gradient nach `locals`.
#[derive(Clone, Debug)]
pub struct Evaluation {
    pub locals: Vec<usize>,
    pub residuals: Vec<Dual>,
}

// ---- Auswertung ---------------------------------------------------------------------------

type V2 = (Dual, Dual);

/// Auswertungskontext. Erster Durchlauf (`collecting`) sammelt die beteiligten Variablen,
/// der zweite baut daraus Dual-Zahlen mit Gradienten fester Länge.
struct Ctx<'a> {
    sk: &'a Sketch,
    x: &'a [f64],
    locals: RefCell<Vec<usize>>,
    collecting: bool,
}

impl<'a> Ctx<'a> {
    fn collecting(sk: &'a Sketch, x: &'a [f64]) -> Ctx<'a> {
        Ctx {
            sk,
            x,
            locals: RefCell::new(Vec::new()),
            collecting: true,
        }
    }

    fn var(&self, i: usize) -> Dual {
        let mut locals = self.locals.borrow_mut();
        let pos = match locals.iter().position(|&l| l == i) {
            Some(p) => p,
            None if self.collecting => {
                locals.push(i);
                locals.len() - 1
            }
            None => unreachable!("Variable wurde im ersten Durchlauf nicht gemeldet"),
        };
        if self.collecting {
            Dual::constant(self.x[i])
        } else {
            Dual::variable(self.x[i], pos, locals.len())
        }
    }

    fn pt(&self, p: PointId) -> V2 {
        let d = &self.sk.points[p];
        (self.var(d.x), self.var(d.y))
    }
}

fn sub2(a: &V2, b: &V2) -> V2 {
    (&a.0 - &b.0, &a.1 - &b.1)
}
fn cross(a: &V2, b: &V2) -> Dual {
    &a.0 * &b.1 - &a.1 * &b.0
}
fn dot(a: &V2, b: &V2) -> Dual {
    &a.0 * &b.0 + &a.1 * &b.1
}
fn length(a: &V2) -> Dual {
    dot(a, a).sqrt()
}
/// Länge, nach unten auf `MIN_LEN` begrenzt (zum Normieren).
fn norm_len(a: &V2) -> Dual {
    let l = length(a);
    if l.v < MIN_LEN {
        Dual::constant(MIN_LEN)
    } else {
        l
    }
}

/// Anfangspunkt und Richtung `b − a` einer (geprüften) Linie.
fn line_dir(ctx: &Ctx, c: CurveId) -> (V2, V2) {
    let CurveKind::Line { a, b } = ctx.sk.curves[c].kind else {
        unreachable!("Typ wurde geprüft")
    };
    let (pa, pb) = (ctx.pt(a), ctx.pt(b));
    let d = sub2(&pb, &pa);
    (pa, d)
}

/// Mittelpunkt und Radius eines (geprüften) Kreises oder Bogens.
fn circle_of(ctx: &Ctx, c: CurveId) -> (V2, Dual) {
    match &ctx.sk.curves[c].kind {
        CurveKind::Circle { center, radius } => (ctx.pt(*center), ctx.var(*radius)),
        CurveKind::Arc { center, start, .. } => {
            let pc = ctx.pt(*center);
            let r = length(&sub2(&ctx.pt(*start), &pc));
            (pc, r)
        }
        _ => unreachable!("Typ wurde geprüft"),
    }
}

/// Mittelpunkt, große Halbachse, deren (nach unten begrenzte) Länge und kleine Halbachse einer
/// (geprüften) Ellipse.
fn ellipse_of(ctx: &Ctx, c: CurveId) -> (V2, V2, Dual, Dual) {
    let CurveKind::Ellipse {
        center,
        ax,
        ay,
        minor,
    } = ctx.sk.curves[c].kind
    else {
        unreachable!("Typ wurde geprüft")
    };
    let m = ctx.pt(center);
    let a = (ctx.var(ax), ctx.var(ay));
    let len = norm_len(&a);
    (m, a, len, ctx.var(minor))
}

/// Punkt und Ableitung nach `t` einer (geprüften) Ellipse oder eines Splines.
fn param_curve(ctx: &Ctx, c: CurveId, t: &Dual) -> (V2, V2) {
    match &ctx.sk.curves[c].kind {
        CurveKind::Ellipse { .. } => {
            let (m, a, len, minor) = ellipse_of(ctx, c);
            ellipse_eval(&m, &a, &len, &minor, t)
        }
        CurveKind::Spline { points } => {
            let pts: Vec<V2> = points.iter().map(|&p| ctx.pt(p)).collect();
            let xs: Vec<Dual> = pts.iter().map(|p| p.0.clone()).collect();
            let ys: Vec<Dual> = pts.iter().map(|p| p.1.clone()).collect();
            let (mx, my) = (
                spline_second_derivatives(&xs),
                spline_second_derivatives(&ys),
            );
            let (x, dx) = spline_eval(&xs, &mx, t);
            let (y, dy) = spline_eval(&ys, &my, t);
            ((x, y), (dx, dy))
        }
        _ => unreachable!("Typ wurde geprüft"),
    }
}

/// Die Zusatzvariable einer parametrischen Abhängigkeit.
fn extra_var(ctx: &Ctx, data: &ConstraintData) -> Dual {
    let Some(t) = data.extra else {
        unreachable!("parametrische Abhängigkeiten werden mit Zusatzvariable angelegt")
    };
    ctx.var(t)
}

fn is_line_kind(k: &CurveKind) -> bool {
    matches!(k, CurveKind::Line { .. })
}

fn is_param_kind(k: &CurveKind) -> bool {
    matches!(k, CurveKind::Ellipse { .. } | CurveKind::Spline { .. })
}

/// Bei einer Linie und einer Ellipse/einem Spline (in beliebiger Reihenfolge): `(Linie, Kurve)`.
fn line_and_param(sk: &Sketch, a: CurveId, b: CurveId) -> Option<(CurveId, CurveId)> {
    let (ka, kb) = (&sk.curves[a].kind, &sk.curves[b].kind);
    if is_line_kind(ka) && is_param_kind(kb) {
        Some((a, b))
    } else if is_param_kind(ka) && is_line_kind(kb) {
        Some((b, a))
    } else {
        None
    }
}

/// Vorzeichenbehafteter Abstand des Punktes p von der Linie (links der Richtung positiv).
fn signed_distance(ctx: &Ctx, p: &V2, line: CurveId) -> Dual {
    let (a, d) = line_dir(ctx, line);
    cross(&d, &sub2(p, &a)) / norm_len(&d)
}

fn eval_kind(ctx: &Ctx, data: &ConstraintData) -> Vec<Dual> {
    use ConstraintKind::*;
    let sk = ctx.sk;
    match &data.kind {
        Coincident(a, b) => {
            let (p, q) = (ctx.pt(*a), ctx.pt(*b));
            vec![&p.0 - &q.0, &p.1 - &q.1]
        }
        PointOnCurve(p, c) => {
            let pp = ctx.pt(*p);
            match &sk.curves[*c].kind {
                CurveKind::Line { .. } => vec![signed_distance(ctx, &pp, *c)],
                CurveKind::Ellipse { .. } => {
                    // implizit: (√((x'/|a|)² + (y'/m)²) − 1)·(|a| + m)/2 in Achskoordinaten
                    let (m, a, len, minor) = ellipse_of(ctx, *c);
                    let v = sub2(&pp, &m);
                    let x = dot(&v, &a) / &len / &len;
                    // kleine Halbachse wie `norm_len` nach unten begrenzt (keine Division durch 0)
                    let minor = if minor.v < MIN_LEN {
                        Dual::constant(MIN_LEN)
                    } else {
                        minor
                    };
                    let y = cross(&a, &v) / &len / &minor;
                    let r = (&x * &x + &y * &y).sqrt();
                    vec![(r - 1.0) * (&len + &minor) * 0.5]
                }
                CurveKind::Spline { .. } => {
                    let t = extra_var(ctx, data);
                    let (q, _) = param_curve(ctx, *c, &t);
                    vec![&q.0 - &pp.0, &q.1 - &pp.1]
                }
                _ => {
                    let (m, r) = circle_of(ctx, *c);
                    vec![length(&sub2(&pp, &m)) - r]
                }
            }
        }
        Horizontal(l) | Vertical(l) => {
            let (a, b) = match l {
                LineOrPoints::Line(c) => {
                    let (a, d) = line_dir(ctx, *c);
                    let b = (&a.0 + &d.0, &a.1 + &d.1);
                    (a, b)
                }
                LineOrPoints::Points(a, b) => (ctx.pt(*a), ctx.pt(*b)),
            };
            if matches!(data.kind, Horizontal(_)) {
                vec![&a.1 - &b.1]
            } else {
                vec![&a.0 - &b.0]
            }
        }
        Parallel(c1, c2) | Perpendicular(c1, c2) => {
            let ((_, d1), (_, d2)) = (line_dir(ctx, *c1), line_dir(ctx, *c2));
            let den = norm_len(&d1) * norm_len(&d2);
            let num = if matches!(data.kind, Parallel(..)) {
                cross(&d1, &d2)
            } else {
                dot(&d1, &d2)
            };
            vec![num / den]
        }
        Tangent(c1, c2) if line_and_param(sk, *c1, *c2).is_some() => {
            let Some((line, curve)) = line_and_param(sk, *c1, *c2) else {
                unreachable!("im Wächter geprüft")
            };
            let t = extra_var(ctx, data);
            let (q, dq) = param_curve(ctx, curve, &t);
            let (_, d) = line_dir(ctx, line);
            let den = norm_len(&dq) * norm_len(&d);
            vec![signed_distance(ctx, &q, line), cross(&dq, &d) / den]
        }
        Tangent(c1, c2) => {
            let s = f64::from(data.branch);
            match (&sk.curves[*c1].kind, &sk.curves[*c2].kind) {
                (CurveKind::Line { .. }, _) => {
                    let (m, r) = circle_of(ctx, *c2);
                    vec![s * signed_distance(ctx, &m, *c1) - r]
                }
                (_, CurveKind::Line { .. }) => {
                    let (m, r) = circle_of(ctx, *c1);
                    vec![s * signed_distance(ctx, &m, *c2) - r]
                }
                _ => {
                    let ((m1, r1), (m2, r2)) = (circle_of(ctx, *c1), circle_of(ctx, *c2));
                    let (s1, s2) = match data.branch {
                        1 => (1.0, -1.0),
                        -1 => (-1.0, 1.0),
                        _ => (1.0, 1.0),
                    };
                    vec![length(&sub2(&m1, &m2)) - (s1 * r1 + s2 * r2)]
                }
            }
        }
        Equal(c1, c2) => {
            if is_line_kind(&sk.curves[*c1].kind) {
                let ((_, d1), (_, d2)) = (line_dir(ctx, *c1), line_dir(ctx, *c2));
                vec![length(&d1) - length(&d2)]
            } else {
                let ((_, r1), (_, r2)) = (circle_of(ctx, *c1), circle_of(ctx, *c2));
                vec![r1 - r2]
            }
        }
        Fixed(p, x, y) => {
            let pp = ctx.pt(*p);
            vec![&pp.0 - *x, &pp.1 - *y]
        }
        Symmetric(a, b, axis) => {
            let (pa, pb) = (ctx.pt(*a), ctx.pt(*b));
            let mid = ((&pa.0 + &pb.0) * 0.5, (&pa.1 + &pb.1) * 0.5);
            let (o, d) = line_dir(ctx, *axis);
            let n = norm_len(&d);
            vec![
                cross(&d, &sub2(&mid, &o)) / &n,
                dot(&d, &sub2(&pb, &pa)) / &n,
            ]
        }
        Midpoint(p, line) => {
            let pp = ctx.pt(*p);
            let (a, d) = line_dir(ctx, *line);
            vec![
                &pp.0 - &(&a.0 + &(&d.0 * 0.5)),
                &pp.1 - &(&a.1 + &(&d.1 * 0.5)),
            ]
        }
        Distance(a, b, v) => vec![length(&sub2(&ctx.pt(*a), &ctx.pt(*b))) - *v],
        DistancePointLine(p, line, v) => {
            let pp = ctx.pt(*p);
            vec![f64::from(data.branch) * signed_distance(ctx, &pp, *line) - *v]
        }
        Radius(c, v) => vec![circle_of(ctx, *c).1 - *v],
        Diameter(c, v) => vec![2.0 * circle_of(ctx, *c).1 - *v],
        Angle(c1, c2, v) => {
            let ((_, d1), (_, d2)) = (line_dir(ctx, *c1), line_dir(ctx, *c2));
            let den = norm_len(&d1) * norm_len(&d2);
            vec![(cross(&d1, &d2) * v.cos() - dot(&d1, &d2) * v.sin()) / den]
        }
        ArcRadius(c) => {
            let CurveKind::Arc { center, start, end } = sk.curves[*c].kind else {
                unreachable!("Typ wurde geprüft")
            };
            let pc = ctx.pt(center);
            vec![length(&sub2(&ctx.pt(end), &pc)) - length(&sub2(&ctx.pt(start), &pc))]
        }
        EllipseMajor(c, v) => {
            let (_, a, _, _) = ellipse_of(ctx, *c);
            vec![length(&a) - *v]
        }
        EllipseMinor(c, v) => vec![ellipse_of(ctx, *c).3 - *v],
        EllipseAngle(c, v) => {
            let (_, a, len, _) = ellipse_of(ctx, *c);
            let dir = (Dual::constant(v.cos()), Dual::constant(v.sin()));
            vec![cross(&a, &dir) / len]
        }
    }
}

/// Endliche Länge oder Koordinate mit Betrag höchstens `MAX_COORD`.
fn finite(v: f64) -> Result<(), SketchError> {
    crate::model::finite(v, tr!("sketch-err-what-value"))
}

/// Endlicher Winkel (ohne Betragsgrenze).
fn finite_angle(v: f64) -> Result<(), SketchError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(SketchError::InvalidInput(tr!("sketch-err-value-finite")))
    }
}

// ---- Sketch-Methoden ----------------------------------------------------------------------

impl Sketch {
    /// Der aktuelle Variablenvektor.
    pub fn variables(&self) -> &[f64] {
        &self.vars
    }

    /// Wertet eine lebende Abhängigkeit (auch interne) am Variablenvektor `x` aus.
    /// `None`, wenn es die Abhängigkeit nicht gibt oder `x` zu kurz ist.
    pub fn evaluate(&self, id: ConstraintId, x: &[f64]) -> Option<Evaluation> {
        if id.index() >= self.constraints.len()
            || !self.constraints[id].alive
            || x.len() < self.vars.len()
        {
            return None;
        }
        let data = &self.constraints[id];
        let first = Ctx::collecting(self, x);
        let _ = eval_kind(&first, data);
        let locals = first.locals.into_inner();
        let second = Ctx {
            sk: self,
            x,
            locals: RefCell::new(locals),
            collecting: false,
        };
        let residuals = eval_kind(&second, data);
        Some(Evaluation {
            locals: second.locals.into_inner(),
            residuals,
        })
    }

    /// Eine lebende, nicht interne Abhängigkeit.
    pub fn constraint(&self, id: ConstraintId) -> Option<&ConstraintData> {
        (id.index() < self.constraints.len()
            && self.constraints[id].alive
            && !self.constraints[id].internal)
            .then(|| &self.constraints[id])
    }

    /// Lebende, nicht interne Abhängigkeiten.
    pub fn constraints(&self) -> impl Iterator<Item = (ConstraintId, &ConstraintData)> {
        self.constraints
            .iter()
            .filter(|(_, c)| c.alive && !c.internal)
    }

    /// Lebende interne Abhängigkeiten (zum Beispiel Bogenradius).
    pub fn internal_constraints(&self) -> impl Iterator<Item = (ConstraintId, &ConstraintData)> {
        self.constraints
            .iter()
            .filter(|(_, c)| c.alive && c.internal)
    }

    pub(crate) fn push_internal(&mut self, kind: ConstraintKind) {
        self.touch_structure();
        self.constraints.push(ConstraintData {
            kind,
            branch: 0,
            driven: false,
            enabled: true,
            internal: true,
            extra: None,
            alive: true,
        });
    }

    fn checked_constraint(&self, id: ConstraintId) -> Result<(), SketchError> {
        if self.constraint(id).is_some() {
            Ok(())
        } else {
            Err(SketchError::UnknownObject)
        }
    }

    pub fn set_driven(&mut self, id: ConstraintId, driven: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].driven = driven;
        self.touch_structure();
        Ok(())
    }

    pub fn set_enabled(&mut self, id: ConstraintId, enabled: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].enabled = enabled;
        self.touch_structure();
        Ok(())
    }

    /// Ändert den Zahlenwert eines Maßes. Die gewählte Lage (`branch`) bleibt erhalten.
    pub fn set_constraint_value(&mut self, id: ConstraintId, v: f64) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        let kind = self.constraints[id]
            .kind
            .with_value(v)
            .ok_or_else(|| SketchError::WrongType(tr!("sketch-not-dimension")))?;
        self.check_refs(&kind)?;
        self.constraints[id].kind = kind;
        self.touch_structure();
        Ok(())
    }

    /// Aktueller Wert eines Maßes (der gespeicherte Wert in `kind` wird ignoriert).
    /// `None` für Abhängigkeiten ohne Zahlenwert oder ungültige Objekte.
    pub fn measure(&self, kind: &ConstraintKind) -> Option<f64> {
        use ConstraintKind::*;
        if !kind.is_dimension() {
            return None;
        }
        self.check_structure(kind).ok()?;
        let ctx = Ctx::collecting(self, &self.vars);
        let v = match kind {
            Distance(a, b, _) => length(&sub2(&ctx.pt(*a), &ctx.pt(*b))).v,
            DistancePointLine(p, l, _) => signed_distance(&ctx, &ctx.pt(*p), *l).v.abs(),
            Radius(c, _) => circle_of(&ctx, *c).1.v,
            Diameter(c, _) => 2.0 * circle_of(&ctx, *c).1.v,
            Angle(c1, c2, _) => {
                let ((_, d1), (_, d2)) = (line_dir(&ctx, *c1), line_dir(&ctx, *c2));
                cross(&d1, &d2).v.atan2(dot(&d1, &d2).v)
            }
            EllipseMajor(c, _) => length(&ellipse_of(&ctx, *c).1).v,
            EllipseMinor(c, _) => ellipse_of(&ctx, *c).3.v,
            EllipseAngle(c, _) => {
                let (_, a, _, _) = ellipse_of(&ctx, *c);
                a.1.v.atan2(a.0.v)
            }
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    /// Prüft Ids, Objekttypen und Zahlenwerte einer Abhängigkeit.
    fn check_refs(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        self.check_structure(kind)?;
        self.check_constraint_value(kind)
    }

    /// Prüft Ids und Objekttypen (ohne die Zahlenwerte der Maße).
    fn check_structure(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        for p in kind.points() {
            self.check_point(p)?;
        }
        for c in kind.curves() {
            self.check_curve(c)?;
        }
        self.check_types(kind)
    }

    /// Prüft nur die Objekttypen und Entartungen; die Ids müssen im Bereich der Arenen liegen
    /// (auch gelöschte Objekte sind erlaubt). Genutzt von `check_structure` und `from_data`.
    pub(crate) fn check_types(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        let is_line = |c: CurveId| is_line_kind(&self.curves[c].kind);
        let is_round = |c: CurveId| {
            matches!(
                self.curves[c].kind,
                CurveKind::Circle { .. } | CurveKind::Arc { .. }
            )
        };
        let wrong = |what: String| -> Result<(), SketchError> { Err(SketchError::WrongType(what)) };
        let degenerate =
            |what: String| -> Result<(), SketchError> { Err(SketchError::Degenerate(what)) };
        use ConstraintKind::*;
        match kind {
            Coincident(a, b) if a == b => degenerate(tr!("sketch-err-point-self")),
            Horizontal(LineOrPoints::Line(c)) | Vertical(LineOrPoints::Line(c)) if !is_line(*c) => {
                wrong(tr!("sketch-err-only-lines"))
            }
            Horizontal(LineOrPoints::Points(a, b)) | Vertical(LineOrPoints::Points(a, b))
                if a == b =>
            {
                degenerate(tr!("sketch-err-two-points"))
            }
            Parallel(a, b) | Perpendicular(a, b) | Angle(a, b, _)
                if !(is_line(*a) && is_line(*b)) =>
            {
                wrong(tr!("sketch-err-only-lines"))
            }
            Parallel(a, b) | Perpendicular(a, b) | Angle(a, b, _) | Equal(a, b) | Tangent(a, b)
                if a == b =>
            {
                degenerate(tr!("sketch-err-two-objects"))
            }
            Tangent(a, b)
                if !((is_line(*a) && is_round(*b))
                    || (is_round(*a) && is_line(*b))
                    || (is_round(*a) && is_round(*b))
                    || line_and_param(self, *a, *b).is_some()) =>
            {
                wrong(tr!("sketch-err-tangent-kinds"))
            }
            Equal(a, b) if !((is_line(*a) && is_line(*b)) || (is_round(*a) && is_round(*b))) => {
                wrong(tr!("sketch-err-equal-kinds"))
            }
            Symmetric(a, b, axis) if a == b || !is_line(*axis) => {
                wrong(tr!("sketch-err-symmetric"))
            }
            Midpoint(_, l) | DistancePointLine(_, l, _) if !is_line(*l) => {
                wrong(tr!("sketch-err-only-lines"))
            }
            Distance(a, b, _) if a == b => degenerate(tr!("sketch-err-two-points")),
            Radius(c, _) | Diameter(c, _) if !is_round(*c) => wrong(tr!("sketch-err-only-circles")),
            ArcRadius(c) if !matches!(self.curves[*c].kind, CurveKind::Arc { .. }) => {
                wrong(tr!("sketch-err-only-arcs"))
            }
            EllipseMajor(c, _) | EllipseMinor(c, _) | EllipseAngle(c, _)
                if !matches!(self.curves[*c].kind, CurveKind::Ellipse { .. }) =>
            {
                wrong(tr!("sketch-err-only-ellipses"))
            }
            _ => Ok(()),
        }
    }

    /// Prüft die Zahlenwerte eines Maßes oder einer festen Lage, ohne Ids und Objekttypen zu
    /// prüfen. Dieselbe Regel gilt für [`Sketch::add_constraint`],
    /// [`Sketch::set_constraint_value`] und die Eingabe in der Oberfläche:
    /// - alle Werte endlich, Längen und Koordinaten betragsmäßig höchstens
    ///   [`MAX_COORD`](crate::model::MAX_COORD);
    /// - Abstand zweier Punkte, Radius, Durchmesser und Halbachsen größer als `tol.linear`
    ///   (ein Abstand null ließe eine Linie zu einem Punkt zusammenfallen; dafür gibt es
    ///   „deckungsgleich“);
    /// - Abstand Punkt–Linie nicht negativ (null heißt: Punkt auf der Linie);
    /// - die kleine Halbachse nicht länger als ein erzwungenes Gegenstück (siehe
    ///   `check_axis_pair`); Winkel beliebig.
    pub fn check_constraint_value(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        use ConstraintKind::*;
        match kind {
            Distance(_, _, v) | Radius(_, v) | Diameter(_, v) => self.check_positive(*v),
            DistancePointLine(_, _, v) => {
                finite(*v)?;
                if *v < 0.0 {
                    Err(SketchError::Degenerate(tr!("sketch-err-distance-negative")))
                } else {
                    Ok(())
                }
            }
            EllipseMajor(e, v) | EllipseMinor(e, v) => {
                self.check_positive(*v)?;
                self.check_axis_pair(kind, *e, *v)
            }
            Angle(_, _, v) | EllipseAngle(_, v) => finite_angle(*v),
            Fixed(_, x, y) => {
                finite(*x)?;
                finite(*y)
            }
            _ => Ok(()),
        }
    }

    /// Endlich, höchstens `MAX_COORD` und größer als `tol.linear`.
    fn check_positive(&self, v: f64) -> Result<(), SketchError> {
        finite(v)?;
        if v <= self.tolerance().linear {
            Err(SketchError::Degenerate(tr!(
                "sketch-err-dimension-positive"
            )))
        } else {
            Ok(())
        }
    }

    /// Die kleine Halbachse darf nicht länger als die große sein: Gibt es auf derselben Ellipse
    /// schon ein erzwungenes (lebendes, nicht nur anzeigendes) Gegenstück, wird ein Wert
    /// abgelehnt, der die Regel verletzt. Ohne Gegenstück bleibt die Regel ungeprüft (die
    /// Geometrie allein ist auch mit vertauschten Rollen eine gültige Ellipse).
    fn check_axis_pair(
        &self,
        kind: &ConstraintKind,
        e: CurveId,
        v: f64,
    ) -> Result<(), SketchError> {
        use ConstraintKind::*;
        let violated = self.constraints.iter().any(|(_, c)| {
            c.alive
                && !c.driven
                && match (kind, &c.kind) {
                    (EllipseMajor(..), EllipseMinor(o, w)) => *o == e && *w > v,
                    (EllipseMinor(..), EllipseMajor(o, w)) => *o == e && v > *w,
                    _ => false,
                }
        });
        if violated {
            let hint = if matches!(kind, EllipseMajor(..)) {
                tr!("sketch-err-hint-minor-smaller")
            } else {
                tr!("sketch-err-hint-major-larger")
            };
            Err(SketchError::Degenerate(tr!(
                "sketch-err-minor-longer",
                hint = hint.to_string()
            )))
        } else {
            Ok(())
        }
    }

    /// Bereich der Zusatzvariablen, wenn sie ein Spline-Parameter ist (Punkt auf Spline,
    /// Tangente Linie–Spline), ausgewertet an den Punktlagen in `x`.
    pub(crate) fn spline_parameter(&self, id: ConstraintId, x: &[f64]) -> Option<SplineParameter> {
        let data = &self.constraints[id];
        let var = data.extra?;
        let points = data
            .kind
            .curves()
            .iter()
            .find_map(|&c| match &self.curves[c].kind {
                CurveKind::Spline { points } => Some(points.clone()),
                _ => None,
            })?;
        let n = points.len();
        let at = |p: PointId| DVec2::new(x[self.points[p].x], x[self.points[p].y]);
        // Spielraum je Ende: `tol.linear` geteilt durch die Sehnenlänge des End-Segments, also
        // etwa `tol.linear` Weglänge auf der Fortsetzung, begrenzt auf
        // [MIN_SPLINE_T_TOL, MAX_SPLINE_T_TOL].
        let slack = |a: PointId, b: PointId| {
            (self.tolerance().linear / at(a).distance(at(b)))
                .clamp(MIN_SPLINE_T_TOL, MAX_SPLINE_T_TOL)
        };
        Some(SplineParameter {
            var,
            end: (n - 1) as f64,
            slack_start: slack(points[0], points[1]),
            slack_end: slack(points[n - 2], points[n - 1]),
        })
    }

    /// Lage bei mehrdeutigen Abhängigkeiten, bestimmt aus der aktuellen Geometrie.
    fn initial_branch(&self, kind: &ConstraintKind) -> i8 {
        use ConstraintKind::*;
        let ctx = Ctx::collecting(self, &self.vars);
        let sign = |v: f64| if v >= 0.0 { 1 } else { -1 };
        match kind {
            Tangent(a, b) if line_and_param(self, *a, *b).is_some() => 0,
            Tangent(a, b) => match (&self.curves[*a].kind, &self.curves[*b].kind) {
                (CurveKind::Line { .. }, _) => {
                    sign(signed_distance(&ctx, &circle_of(&ctx, *b).0, *a).v)
                }
                (_, CurveKind::Line { .. }) => {
                    sign(signed_distance(&ctx, &circle_of(&ctx, *a).0, *b).v)
                }
                _ => {
                    let ((m1, r1), (m2, r2)) = (circle_of(&ctx, *a), circle_of(&ctx, *b));
                    let d = length(&sub2(&m1, &m2)).v;
                    let (r1, r2) = (r1.v, r2.v);
                    let candidates = [
                        (0, (d - (r1 + r2)).abs()),
                        (1, (d - (r1 - r2)).abs()),
                        (-1, (d - (r2 - r1)).abs()),
                    ];
                    candidates
                        .iter()
                        .min_by(|x, y| x.1.total_cmp(&y.1))
                        .map_or(0, |c| c.0)
                }
            },
            DistancePointLine(p, l, _) => sign(signed_distance(&ctx, &ctx.pt(*p), *l).v),
            _ => 0,
        }
    }

    /// Abgetastete Form einer Ellipse oder eines Splines (sonst `None`).
    fn sampled(&self, c: CurveId) -> Option<Sampled> {
        match &self.curves[c].kind {
            CurveKind::Ellipse {
                center,
                ax,
                ay,
                minor,
            } => Some(Sampled::ellipse(
                self.pos(*center),
                DVec2::new(self.vars[*ax], self.vars[*ay]),
                self.vars[*minor],
            )),
            CurveKind::Spline { points } => {
                let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
                Some(Sampled::spline(&xy))
            }
            _ => None,
        }
    }

    /// Braucht die (geprüfte) Abhängigkeit eine Zusatzvariable? Dieselbe Regel wie in
    /// `extra_start`: Punkt auf Spline, Tangente zwischen Linie und Ellipse/Spline.
    pub(crate) fn needs_extra(&self, kind: &ConstraintKind) -> bool {
        match kind {
            ConstraintKind::PointOnCurve(_, c) => {
                matches!(self.curves[*c].kind, CurveKind::Spline { .. })
            }
            ConstraintKind::Tangent(a, b) => line_and_param(self, *a, *b).is_some(),
            _ => false,
        }
    }

    /// Startwert der Zusatzvariablen einer (geprüften) parametrischen Abhängigkeit: der
    /// nächstgelegene Kurvenparameter. `None`, wenn die Abhängigkeit keine braucht.
    fn extra_start(&self, kind: &ConstraintKind) -> Option<f64> {
        match kind {
            ConstraintKind::PointOnCurve(p, c)
                if matches!(self.curves[*c].kind, CurveKind::Spline { .. }) =>
            {
                Some(self.sampled(*c)?.nearest_to_point(self.pos(*p)))
            }
            ConstraintKind::Tangent(a, b) => {
                let (line, curve) = line_and_param(self, *a, *b)?;
                let CurveKind::Line { a: la, b: lb } = self.curves[line].kind else {
                    return None;
                };
                let o = self.pos(la);
                Some(self.sampled(curve)?.tangent_to_line(o, self.pos(lb) - o))
            }
            _ => None,
        }
    }

    /// Legt eine Abhängigkeit an. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn add_constraint(&mut self, kind: ConstraintKind) -> Result<ConstraintId, SketchError> {
        if matches!(kind, ConstraintKind::ArcRadius(_)) {
            return Err(SketchError::WrongType(tr!(
                "sketch-err-internal-constraint"
            )));
        }
        self.check_refs(&kind)?;
        let branch = self.initial_branch(&kind);
        // Erst nach allen Prüfungen anlegen, damit ein Fehler nichts hinterlässt.
        let extra = self.extra_start(&kind).map(|t| self.new_var(t));
        self.touch_structure();
        Ok(self.constraints.push(ConstraintData {
            kind,
            branch,
            driven: false,
            enabled: true,
            internal: false,
            extra,
            alive: true,
        }))
    }

    /// Entfernt eine (nicht interne) Abhängigkeit.
    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].alive = false;
        self.touch_structure();
        Ok(())
    }

    /// Entfernt die Kurve samt allen Abhängigkeiten, die sie benutzen (auch interne).
    pub fn remove_curve(&mut self, c: CurveId) -> Result<(), SketchError> {
        self.check_curve(c)?;
        self.touch_structure();
        self.curves[c].alive = false;
        let dead: Vec<ConstraintId> = self
            .constraints
            .iter()
            .filter(|(_, k)| k.alive && k.kind.curves().contains(&c))
            .map(|(i, _)| i)
            .collect();
        for i in dead {
            self.constraints[i].alive = false;
        }
        Ok(())
    }

    /// Entfernt den Punkt samt allen Kurven und Abhängigkeiten, die ihn benutzen.
    pub fn remove_point(&mut self, p: PointId) -> Result<(), SketchError> {
        self.check_point(p)?;
        self.touch_structure();
        self.points[p].alive = false;
        let curves: Vec<CurveId> = self
            .curves()
            .filter(|(_, d)| d.kind.points().contains(&p))
            .map(|(i, _)| i)
            .collect();
        for c in curves {
            self.remove_curve(c)?;
        }
        let dead: Vec<ConstraintId> = self
            .constraints
            .iter()
            .filter(|(_, k)| k.alive && k.kind.points().contains(&p))
            .map(|(i, _)| i)
            .collect();
        for i in dead {
            self.constraints[i].alive = false;
        }
        Ok(())
    }

    /// Entfernt Punkte, die weder eine Kurve noch eine (nicht interne) Abhängigkeit benutzt.
    /// Gibt die Zahl der entfernten Punkte zurück.
    pub fn prune_unused_points(&mut self) -> usize {
        let all: Vec<PointId> = self.points().map(|(i, _)| i).collect();
        self.prune_points(&all)
    }

    /// Entfernt diejenigen der Punkte `candidates`, die (noch) leben und weder eine Kurve noch
    /// eine (nicht interne) Abhängigkeit benutzt; andere freie Punkte bleiben. Gibt die Zahl
    /// der entfernten Punkte zurück; unbekannte und doppelte Ids werden übergangen.
    pub fn prune_points(&mut self, candidates: &[PointId]) -> usize {
        let mut used = std::collections::HashSet::new();
        for (_, c) in self.curves() {
            used.extend(c.kind.points());
        }
        for (_, k) in self.constraints() {
            used.extend(k.kind.points());
        }
        let mut removed = 0;
        for &p in candidates {
            if self.check_point(p).is_ok() && !used.contains(&p) {
                self.points[p].alive = false;
                removed += 1;
            }
        }
        if removed > 0 {
            self.touch_structure();
        }
        removed
    }
}
