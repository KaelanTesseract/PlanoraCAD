//! Abhängigkeiten und Maße. Jede Abhängigkeit liefert Residuen als `Dual`, also mit exakten
//! Ableitungen nach den beteiligten Variablen.

use crate::dual::Dual;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_kernel::Id;
use std::cell::RefCell;

/// Mindestlänge für Richtungsvektoren beim Normieren (verhindert Division durch null).
const MIN_LEN: f64 = 1e-9;

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
        )
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
    pub branch: i8,
    /// Nur anzeigen, nicht erzwingen.
    pub driven: bool,
    pub enabled: bool,
    pub internal: bool,
    /// Zusätzliche Variable (Index im Variablenvektor) für spätere Abhängigkeiten; vorerst immer `None`.
    pub extra: Option<usize>,
    pub(crate) alive: bool,
}
pub type ConstraintId = Id<ConstraintData>;

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

fn is_line_kind(k: &CurveKind) -> bool {
    matches!(k, CurveKind::Line { .. })
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
            if is_line_kind(&sk.curves[*c].kind) {
                vec![signed_distance(ctx, &pp, *c)]
            } else {
                let (m, r) = circle_of(ctx, *c);
                vec![length(&sub2(&pp, &m)) - r]
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
    }
}

fn finite(v: f64) -> Result<(), SketchError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(SketchError::InvalidInput(
            "Der Wert muss eine endliche Zahl sein".into(),
        ))
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
        Ok(())
    }

    pub fn set_enabled(&mut self, id: ConstraintId, enabled: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].enabled = enabled;
        Ok(())
    }

    /// Ändert den Zahlenwert eines Maßes. Die gewählte Lage (`branch`) bleibt erhalten.
    pub fn set_constraint_value(&mut self, id: ConstraintId, v: f64) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        let mut kind = self.constraints[id].kind.clone();
        match &mut kind {
            ConstraintKind::Distance(_, _, x)
            | ConstraintKind::DistancePointLine(_, _, x)
            | ConstraintKind::Angle(_, _, x)
            | ConstraintKind::Radius(_, x)
            | ConstraintKind::Diameter(_, x) => *x = v,
            _ => return Err(SketchError::WrongType("Das ist kein Maß".into())),
        }
        self.check_refs(&kind)?;
        self.constraints[id].kind = kind;
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
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    /// Prüft Ids, Objekttypen und Zahlenwerte einer Abhängigkeit.
    fn check_refs(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        self.check_structure(kind)?;
        self.check_values(kind)
    }

    /// Prüft Ids und Objekttypen (ohne die Zahlenwerte der Maße).
    fn check_structure(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        for p in kind.points() {
            self.check_point(p)?;
        }
        for c in kind.curves() {
            self.check_curve(c)?;
        }
        let is_line = |c: CurveId| is_line_kind(&self.curves[c].kind);
        let is_round = |c: CurveId| {
            matches!(
                self.curves[c].kind,
                CurveKind::Circle { .. } | CurveKind::Arc { .. }
            )
        };
        let wrong = |what: &str| -> Result<(), SketchError> {
            Err(SketchError::WrongType(what.to_string()))
        };
        let degenerate = |what: &str| -> Result<(), SketchError> {
            Err(SketchError::Degenerate(what.to_string()))
        };
        use ConstraintKind::*;
        match kind {
            Coincident(a, b) if a == b => {
                degenerate("Ein Punkt kann nicht mit sich selbst zusammenfallen")
            }
            PointOnCurve(_, c) if !(is_line(*c) || is_round(*c)) => {
                wrong("Ein Punkt kann nur auf einer Linie, einem Kreis oder einem Bogen liegen")
            }
            Horizontal(LineOrPoints::Line(c)) | Vertical(LineOrPoints::Line(c)) if !is_line(*c) => {
                wrong("Das gilt nur für Linien")
            }
            Horizontal(LineOrPoints::Points(a, b)) | Vertical(LineOrPoints::Points(a, b))
                if a == b =>
            {
                degenerate("Zwei verschiedene Punkte sind nötig")
            }
            Parallel(a, b) | Perpendicular(a, b) | Angle(a, b, _)
                if !(is_line(*a) && is_line(*b)) =>
            {
                wrong("Das gilt nur für Linien")
            }
            Parallel(a, b) | Perpendicular(a, b) | Angle(a, b, _) | Equal(a, b) | Tangent(a, b)
                if a == b =>
            {
                degenerate("Zwei verschiedene Objekte sind nötig")
            }
            Tangent(a, b)
                if !((is_line(*a) && is_round(*b))
                    || (is_round(*a) && is_line(*b))
                    || (is_round(*a) && is_round(*b))) =>
            {
                wrong("Tangential geht nur zwischen Linie und Kreis/Bogen oder zwei Kreisen/Bögen")
            }
            Equal(a, b) if !((is_line(*a) && is_line(*b)) || (is_round(*a) && is_round(*b))) => {
                wrong("Gleich geht nur für zwei Linien oder zwei Kreise/Bögen")
            }
            Symmetric(a, b, axis) if a == b || !is_line(*axis) => {
                wrong("Symmetrie braucht zwei verschiedene Punkte und eine Linie als Achse")
            }
            Midpoint(_, l) | DistancePointLine(_, l, _) if !is_line(*l) => {
                wrong("Das gilt nur für Linien")
            }
            Distance(a, b, _) if a == b => degenerate("Zwei verschiedene Punkte sind nötig"),
            Radius(c, _) | Diameter(c, _) if !is_round(*c) => {
                wrong("Das gilt nur für Kreise und Bögen")
            }
            ArcRadius(c) if !matches!(self.curves[*c].kind, CurveKind::Arc { .. }) => {
                wrong("Das gilt nur für Bögen")
            }
            _ => Ok(()),
        }
    }

    /// Prüft die Zahlenwerte (endlich, Abstände ≥ 0, Radien > `tol.linear`).
    fn check_values(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        use ConstraintKind::*;
        match kind {
            Distance(_, _, v) | DistancePointLine(_, _, v) => {
                finite(*v)?;
                if *v < 0.0 {
                    Err(SketchError::Degenerate(
                        "Der Abstand darf nicht negativ sein".into(),
                    ))
                } else {
                    Ok(())
                }
            }
            Radius(_, v) | Diameter(_, v) => {
                finite(*v)?;
                if *v <= self.tolerance().linear {
                    Err(SketchError::Degenerate(
                        "Das Maß muss größer als null sein".into(),
                    ))
                } else {
                    Ok(())
                }
            }
            Angle(_, _, v) => finite(*v),
            Fixed(_, x, y) => {
                finite(*x)?;
                finite(*y)
            }
            _ => Ok(()),
        }
    }

    /// Lage bei mehrdeutigen Abhängigkeiten, bestimmt aus der aktuellen Geometrie.
    fn initial_branch(&self, kind: &ConstraintKind) -> i8 {
        use ConstraintKind::*;
        let ctx = Ctx::collecting(self, &self.vars);
        let sign = |v: f64| if v >= 0.0 { 1 } else { -1 };
        match kind {
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

    /// Legt eine Abhängigkeit an. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn add_constraint(&mut self, kind: ConstraintKind) -> Result<ConstraintId, SketchError> {
        if matches!(kind, ConstraintKind::ArcRadius(_)) {
            return Err(SketchError::WrongType(
                "Diese Abhängigkeit wird intern angelegt".into(),
            ));
        }
        self.check_refs(&kind)?;
        let branch = self.initial_branch(&kind);
        Ok(self.constraints.push(ConstraintData {
            kind,
            branch,
            driven: false,
            enabled: true,
            internal: false,
            extra: None,
            alive: true,
        }))
    }

    /// Entfernt eine (nicht interne) Abhängigkeit.
    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].alive = false;
        Ok(())
    }

    /// Entfernt die Kurve samt allen Abhängigkeiten, die sie benutzen (auch interne).
    pub fn remove_curve(&mut self, c: CurveId) -> Result<(), SketchError> {
        self.check_curve(c)?;
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
        let mut used = std::collections::HashSet::new();
        for (_, c) in self.curves() {
            used.extend(c.kind.points());
        }
        for (_, k) in self.constraints() {
            used.extend(k.kind.points());
        }
        let unused: Vec<PointId> = self
            .points()
            .map(|(i, _)| i)
            .filter(|i| !used.contains(i))
            .collect();
        for p in &unused {
            self.points[*p].alive = false;
        }
        unused.len()
    }
}
