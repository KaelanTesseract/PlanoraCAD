//! Abhängigkeiten und Maße. Jede Abhängigkeit liefert Residuen als `Dual`, also mit exakten
//! Ableitungen nach den beteiligten Variablen.

use crate::constraint3d::{dot3c, ellipse_axis3, ellipse_residual3, length3, sub3};
use crate::curves::{
    CONIC_KNOTS, Sampled, conic_weight, ellipse_eval, nurbs_eval_dual, spline_eval,
    spline_second_derivatives,
};
use crate::dual::Dual;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, HandleCurvature, PointId, Sketch};
use crate::pattern::{PATTERN_MAX_INSTANCES, PatternId, PatternProblem, PatternSlot};
use crate::spline_handles::{HANDLE_MAX_OPENING, handle_kappa, hermite_eval_dual, nodes_of};
use planora_i18n::tr;
use planora_kernel::Id;
use planora_math::DVec2;
use std::cell::RefCell;

pub use crate::constraint3d::Axis3;

/// Mindestlänge für Richtungsvektoren beim Normieren (verhindert Division durch null).
pub(crate) const MIN_LEN: f64 = 1e-9;
/// Untergrenze des Spielraums (in Parametereinheiten), mit dem ein Spline-Parameter über
/// `[0, n − 1]` hinausragen darf (sehr lange End-Segmente).
const MIN_SPLINE_T_TOL: f64 = 1e-12;
/// Obergrenze dieses Spielraums (zu einem Punkt zusammengezogene End-Segmente).
const MAX_SPLINE_T_TOL: f64 = 1e-3;

/// Bereich des Spline-Parameters einer parametrischen Abhängigkeit: Variable, Bereich `(lo, hi)`
/// (beim Spline durch Stützpunkte `(0, n − 1)`) und erlaubter Überstand an beiden Enden.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SplineParameter {
    pub var: usize,
    pub lo: f64,
    pub hi: f64,
    pub slack_start: f64,
    pub slack_end: f64,
}

impl SplineParameter {
    /// Liegt `t` im (um den Spielraum erweiterten) Bereich? NaN liegt nie darin.
    pub fn contains(&self, t: f64) -> bool {
        t >= self.lo - self.slack_start && t <= self.hi + self.slack_end
    }
}

/// Eine Linie oder zwei Punkte (für waagerecht/senkrecht).
#[derive(Clone, Debug, PartialEq)]
pub enum LineOrPoints {
    Line(CurveId),
    Points(PointId, PointId),
}

/// Ergebnis von [`Sketch::toggle_fixed`]: so viele `Fixed` wurden angelegt oder gelöst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixToggle {
    Fixed(usize),
    Released(usize),
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
    /// Intern: Start und Ende eines Ellipsenbogens liegen auf seiner Ellipse.
    EllipticArcEnds(CurveId),
    /// Zwei Linien auf derselben Geraden, auch mit Abstand zwischen ihnen (ab Format 15).
    Collinear(CurveId, CurveId),
    /// Glatter Übergang: Kurve, ihr Ende, Kurve, ihr Ende. Mindestens eine Kurve ist ein Spline,
    /// die andere Linie, Bogen oder Spline (ab Format 15).
    Smooth(CurveId, PointId, CurveId, PointId),
    /// Waagerechter Abstand `|b − a|ₓ` zweier Punkte; die Seite steht in `branch` (ab Format 15).
    DistanceX(PointId, PointId, f64),
    /// Senkrechter Abstand `|b − a|ᵧ` zweier Punkte; die Seite steht in `branch` (ab Format 15).
    DistanceY(PointId, PointId, f64),
    /// Doppelter Abstand eines Punktes von einer Mittellinie (Durchmesser eines Drehteils, ab
    /// Format 15). Angelegt wird es nur an Mittellinien.
    CenterlineDiameter(PointId, CurveId, f64),
    /// Gesamtlänge eines geraden Langlochs: Abstand der Mitten plus Durchmesser der Kappe (ab
    /// Format 15).
    SlotOverall(PointId, PointId, CurveId, f64),
    /// Wert eines Musters (Anzahl, Abstand oder Winkel): ein Maß ohne Zeile, das nur
    /// [`Sketch::add_pattern`] anlegt (ab Format 15). Die Lage der Exemplare folgt daraus, siehe
    /// `pattern.rs`.
    PatternValue(PatternId, PatternSlot, f64),
    /// Intern: Krümmungsgriff des Splines am Stützpunkt (Skizzen 7, B2). Zwei Zeilen: Die Mitte des
    /// Griff-Bogens liegt auf der Normalen des Tangentengriffs, und die Öffnung des Bogens ist
    /// `min(L / r, 2,5 rad)`. Es bleibt der Radius als einziger Freiheitsgrad.
    SplineCurvatureHandle(CurveId, PointId),
    /// Intern (Skizzen 10): Eine ebene Kurve mit eigener Normale bleibt in ihrer Ebene durch den
    /// Mittelpunkt. Beim Bogen liegen Start und Ende darin, bei der Ellipse steht die große
    /// Halbachse senkrecht zur Normale, beim Ellipsenbogen gilt beides. Der Kreis braucht sie
    /// nicht: Sein Umfang liegt über die Normale ohnehin in der Ebene.
    InPlane(CurveId),
    /// Linie oder zwei Punkte parallel zur z-Achse der Skizze (Skizzen 10): die dritte Art neben
    /// waagerecht (x) und senkrecht (y), nur an 3D-Geometrie (ab Format 20).
    AlongZ(LineOrPoints),
    /// Fester Punkt im Raum (Skizzen 10): x, y und z. `Fixed` bleibt die 2D-Fassung; an einem
    /// 3D-Punkt legt das Fixieren `Fixed3` an (ab Format 20).
    Fixed3(PointId, f64, f64, f64),
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
                | ConstraintKind::DistanceX(..)
                | ConstraintKind::DistanceY(..)
                | ConstraintKind::CenterlineDiameter(..)
                | ConstraintKind::SlotOverall(..)
                | ConstraintKind::PatternValue(..)
        )
    }

    /// Braucht ein Eintrag dieser Art Dateiformat 15? Gilt für die Arten aus Skizzen 5.
    pub fn needs_format_15(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Collinear(..)
                | ConstraintKind::Smooth(..)
                | ConstraintKind::DistanceX(..)
                | ConstraintKind::DistanceY(..)
                | ConstraintKind::CenterlineDiameter(..)
                | ConstraintKind::SlotOverall(..)
                | ConstraintKind::PatternValue(..)
        )
    }

    /// Lässt sich die Art an 3D-Geometrie setzen (Skizzen 10)? Die Liste von Fusion:
    /// Deckungsgleich (auch Konzentrisch), Punkt auf Kurve, Waagerecht/Senkrecht/`AlongZ`,
    /// Parallel, Rechtwinklig, Kollinear, Tangential, Gleich, Fixieren, Mittelpunkt, dazu die Maße
    /// Abstand, Radius, Durchmesser und Winkel (Annahme A2) und die internen Einträge der ebenen
    /// Kurven. Alle übrigen sind an 3D-Geometrie gesperrt.
    pub fn allowed_in_3d(&self) -> bool {
        use ConstraintKind::*;
        matches!(
            self,
            Coincident(..)
                | PointOnCurve(..)
                | Horizontal(..)
                | Vertical(..)
                | AlongZ(..)
                | Parallel(..)
                | Perpendicular(..)
                | Collinear(..)
                | Tangent(..)
                | Equal(..)
                | Fixed(..)
                | Fixed3(..)
                | Midpoint(..)
                | Distance(..)
                | Radius(..)
                | Diameter(..)
                | Angle(..)
                | ArcRadius(..)
                | EllipticArcEnds(..)
                | InPlane(..)
        )
    }

    /// Ist das ein Radius oder Durchmesser?
    pub fn is_radius(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Radius(..) | ConstraintKind::Diameter(..)
        )
    }

    /// Der Wert, wenn das ein Längenmaß ist (für den Falt-Wächter, siehe
    /// [`crate::shapes::folded_over`]).
    pub fn length_value(&self) -> Option<f64> {
        use ConstraintKind::*;
        match *self {
            Distance(_, _, v)
            | DistancePointLine(_, _, v)
            | Radius(_, v)
            | Diameter(_, v)
            | DistanceX(_, _, v)
            | DistanceY(_, _, v)
            | CenterlineDiameter(_, _, v)
            | SlotOverall(_, _, _, v) => Some(v),
            _ => None,
        }
    }

    /// Der gespeicherte Zahlenwert eines Maßes (dieselben Arten wie
    /// [`with_value`](Self::with_value)); `None` für Abhängigkeiten ohne Zahlenwert.
    pub fn value(&self) -> Option<f64> {
        use ConstraintKind::*;
        match *self {
            Distance(_, _, x)
            | DistancePointLine(_, _, x)
            | Angle(_, _, x)
            | Radius(_, x)
            | Diameter(_, x)
            | EllipseMajor(_, x)
            | EllipseMinor(_, x)
            | EllipseAngle(_, x)
            | DistanceX(_, _, x)
            | DistanceY(_, _, x)
            | CenterlineDiameter(_, _, x)
            | SlotOverall(_, _, _, x)
            | PatternValue(_, _, x) => Some(x),
            _ => None,
        }
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
            | EllipseAngle(_, x)
            | DistanceX(_, _, x)
            | DistanceY(_, _, x)
            | CenterlineDiameter(_, _, x)
            | SlotOverall(_, _, _, x)
            | PatternValue(_, _, x) => *x = v,
            _ => return None,
        }
        Some(kind)
    }

    /// Die direkt beteiligten Punkte.
    pub fn points(&self) -> Vec<PointId> {
        use ConstraintKind::*;
        match self {
            Coincident(a, b)
            | Distance(a, b, _)
            | Symmetric(a, b, _)
            | DistanceX(a, b, _)
            | DistanceY(a, b, _)
            | SlotOverall(a, b, ..)
            | Smooth(_, a, _, b) => vec![*a, *b],
            Horizontal(LineOrPoints::Points(a, b))
            | Vertical(LineOrPoints::Points(a, b))
            | AlongZ(LineOrPoints::Points(a, b)) => {
                vec![*a, *b]
            }
            PointOnCurve(p, _)
            | Fixed(p, ..)
            | Fixed3(p, ..)
            | Midpoint(p, _)
            | DistancePointLine(p, ..)
            | CenterlineDiameter(p, ..)
            | SplineCurvatureHandle(_, p) => vec![*p],
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
            | EllipticArcEnds(c)
            | InPlane(c)
            | EllipseMajor(c, _)
            | EllipseMinor(c, _)
            | EllipseAngle(c, _)
            | Symmetric(_, _, c)
            | Horizontal(LineOrPoints::Line(c))
            | Vertical(LineOrPoints::Line(c))
            | AlongZ(LineOrPoints::Line(c))
            | CenterlineDiameter(_, c, _)
            | SlotOverall(_, _, c, _)
            | SplineCurvatureHandle(c, _) => vec![*c],
            Parallel(a, b)
            | Perpendicular(a, b)
            | Tangent(a, b)
            | Equal(a, b)
            | Angle(a, b, _)
            | Collinear(a, b)
            | Smooth(a, _, b, _) => vec![*a, *b],
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConstraintData {
    pub kind: ConstraintKind,
    /// Gewählte Lage bei mehrdeutigen Abhängigkeiten (Tangente, Abstand Punkt–Linie).
    pub(crate) branch: i8,
    /// Tangente an einem gemeinsamen Endpunkt: Vorzeichen, unter dem die Richtung gilt, aus der
    /// Geometrie beim Anlegen, Laden oder Ändern der Kurve (`0` sonst). Steht nicht in der Datei.
    pub(crate) shared_sign: i8,
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
use crate::constraint3d::V3;

/// Auswertungskontext. Erster Durchlauf (`collecting`) sammelt die beteiligten Variablen,
/// der zweite baut daraus Dual-Zahlen mit Gradienten fester Länge.
pub(crate) struct Ctx<'a> {
    pub(crate) sk: &'a Sketch,
    x: &'a [f64],
    locals: RefCell<Vec<usize>>,
    collecting: bool,
}

impl<'a> Ctx<'a> {
    pub(crate) fn collecting(sk: &'a Sketch, x: &'a [f64]) -> Ctx<'a> {
        Ctx {
            sk,
            x,
            locals: RefCell::new(Vec::new()),
            collecting: true,
        }
    }

    pub(crate) fn var(&self, i: usize) -> Dual {
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

    pub(crate) fn pt(&self, p: PointId) -> V2 {
        let d = &self.sk.points[p];
        (self.var(d.x), self.var(d.y))
    }

    /// Lage eines Punktes im Raum (Skizzen 10); ein 2D-Punkt hat z als Konstante 0 ohne
    /// Gradient.
    pub(crate) fn pt3(&self, p: PointId) -> V3 {
        let d = &self.sk.points[p];
        let z = d.z.map_or_else(|| Dual::constant(0.0), |i| self.var(i));
        (self.var(d.x), self.var(d.y), z)
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
        unreachable!("type was checked")
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
        _ => unreachable!("type was checked"),
    }
}

/// Mittelpunkt, große Halbachse, deren (nach unten begrenzte) Länge und kleine Halbachse einer
/// (geprüften) Ellipse oder eines Ellipsenbogens.
fn ellipse_of(ctx: &Ctx, c: CurveId) -> (V2, V2, Dual, Dual) {
    let (CurveKind::Ellipse {
        center,
        ax,
        ay,
        minor,
        ..
    }
    | CurveKind::EllipticArc {
        center,
        ax,
        ay,
        minor,
        ..
    }) = ctx.sk.curves[c].kind
    else {
        unreachable!("type was checked")
    };
    let m = ctx.pt(center);
    let a = (ctx.var(ax), ctx.var(ay));
    let len = norm_len(&a);
    (m, a, len, ctx.var(minor))
}

/// Abstand des Punktes `p` von der (geprüften) Ellipse oder dem Ellipsenbogen `c` in impliziter
/// Form: `(√((x'/|a|)² + (y'/m)²) − 1)·(|a| + m)/2` in Achskoordinaten.
fn ellipse_residual(ctx: &Ctx, c: CurveId, p: &V2) -> Dual {
    let (m, a, len, minor) = ellipse_of(ctx, c);
    let v = sub2(p, &m);
    let x = dot(&v, &a) / &len / &len;
    // kleine Halbachse wie `norm_len` nach unten begrenzt (keine Division durch 0)
    let minor = if minor.v < MIN_LEN {
        Dual::constant(MIN_LEN)
    } else {
        minor
    };
    let y = cross(&a, &v) / &len / &minor;
    let r = (&x * &x + &y * &y).sqrt();
    (r - 1.0) * (&len + &minor) * 0.5
}

/// Punkt und Ableitung nach `t` einer (geprüften) Ellipse, eines Ellipsenbogens (auf seiner
/// ganzen Ellipse), eines Splines oder einer Freiformkurve.
fn param_curve(ctx: &Ctx, c: CurveId, t: &Dual) -> (V2, V2) {
    match &ctx.sk.curves[c].kind {
        CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
            let (m, a, len, minor) = ellipse_of(ctx, c);
            ellipse_eval(&m, &a, &len, &minor, t)
        }
        CurveKind::Spline { points, handles } if !handles.is_empty() => {
            hermite_eval_dual(&nodes_of(points, handles, |p| ctx.pt(p)), t)
        }
        CurveKind::Spline { points, .. } => {
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
        CurveKind::ControlSpline {
            points,
            degree,
            knots,
        } => {
            let pts: Vec<V2> = points.iter().map(|&p| ctx.pt(p)).collect();
            nurbs_eval_dual(usize::from(*degree), knots, None, &pts, t)
        }
        CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        } => {
            let pts = [ctx.pt(*start), ctx.pt(*apex), ctx.pt(*end)];
            let weights = [1.0, conic_weight(*rho), 1.0];
            nurbs_eval_dual(2, &CONIC_KNOTS, Some(&weights), &pts, t)
        }
        CurveKind::FixedSpline { curve, .. } => {
            // Die Kurve steht fest: Zahlen ohne Gradient, nur `t` läuft.
            let pts: Vec<V2> = curve
                .control_points()
                .iter()
                .map(|q| (Dual::constant(q.x), Dual::constant(q.y)))
                .collect();
            nurbs_eval_dual(
                curve.degree(),
                curve.knots().knots(),
                Some(curve.weights()),
                &pts,
                t,
            )
        }
        _ => unreachable!("type was checked"),
    }
}

/// Die Zusatzvariable einer parametrischen Abhängigkeit.
pub(crate) fn extra_var(ctx: &Ctx, data: &ConstraintData) -> Dual {
    let Some(t) = data.extra else {
        unreachable!("parametric constraints are created with an extra variable")
    };
    ctx.var(t)
}

pub(crate) fn is_line_kind(k: &CurveKind) -> bool {
    matches!(k, CurveKind::Line { .. })
}

fn is_param_kind(k: &CurveKind) -> bool {
    matches!(
        k,
        CurveKind::Ellipse { .. }
            | CurveKind::EllipticArc { .. }
            | CurveKind::Spline { .. }
            | CurveKind::ControlSpline { .. }
            | CurveKind::Conic { .. }
            | CurveKind::FixedSpline { .. }
    )
}

/// Kurven, auf denen „Punkt auf Kurve“ einen Kurvenparameter als Zusatzvariable braucht.
fn on_curve_needs_parameter(k: &CurveKind) -> bool {
    matches!(
        k,
        CurveKind::Spline { .. }
            | CurveKind::ControlSpline { .. }
            | CurveKind::Conic { .. }
            | CurveKind::FixedSpline { .. }
    )
}

fn is_ellipse_kind(k: &CurveKind) -> bool {
    matches!(k, CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. })
}

/// Bei einer Linie und einer Ellipse/einem Ellipsenbogen/einem Spline (in beliebiger
/// Reihenfolge): `(Linie, Kurve)`.
pub(crate) fn line_and_param(sk: &Sketch, a: CurveId, b: CurveId) -> Option<(CurveId, CurveId)> {
    let (ka, kb) = (&sk.curves[a].kind, &sk.curves[b].kind);
    if is_line_kind(ka) && is_param_kind(kb) {
        Some((a, b))
    } else if is_param_kind(ka) && is_line_kind(kb) {
        Some((b, a))
    } else {
        None
    }
}

/// Winkel (rad), um den eine Tangente an einem gemeinsamen Endpunkt danebenliegen darf, ohne als
/// Widerspruch zu gelten. Eine frühere Zeile hielt die Richtung nur in zweiter Ordnung; der Solver
/// blieb etwa 5e-5 rad davor stehen (Mitte rund 3e-4 mm daneben bei r = 5), und „fest“ oder ein
/// gemessenes Maß haben diese Lage gespeichert. Solche alten Skizzen bleiben lösbar.
pub(crate) const SHARED_END_TANGENT_DRIFT: f64 = 1e-3;

/// Gemeinsamer Endpunkt zweier Linien oder Bögen.
pub(crate) fn shared_end(sk: &Sketch, a: CurveId, b: CurveId) -> Option<PointId> {
    let ends = |c: CurveId| match sk.curves[c].kind {
        CurveKind::Line { a, b } => Some([a, b]),
        CurveKind::Arc { start, end, .. } => Some([start, end]),
        _ => None,
    };
    let (ea, eb) = (ends(a)?, ends(b)?);
    ea.into_iter().find(|p| eb.contains(p))
}

/// Sinus- und Kosinusanteil der Tangente `c1`–`c2` an ihrem gemeinsamen Endpunkt `p`: Der Sinusanteil
/// ist null, wenn sie sich berühren, der Kosinusanteil sagt, in welcher der beiden Lagen.
fn shared_end_parts(ctx: &Ctx, c1: CurveId, c2: CurveId, p: PointId) -> (Dual, Dual) {
    let sk = ctx.sk;
    let at = ctx.pt(p);
    let line_arc = |line: CurveId, arc: CurveId| {
        let (m, _) = circle_of(ctx, arc);
        let (_, d) = line_dir(ctx, line);
        let radial = sub2(&at, &m);
        (dot(&d, &radial), cross(&d, &radial))
    };
    match (&sk.curves[c1].kind, &sk.curves[c2].kind) {
        (CurveKind::Line { .. }, _) => line_arc(c1, c2),
        (_, CurveKind::Line { .. }) => line_arc(c2, c1),
        _ => {
            let ((m1, _), (m2, _)) = (circle_of(ctx, c1), circle_of(ctx, c2));
            let (u, v) = (sub2(&m1, &at), sub2(&m2, &at));
            (cross(&u, &v), dot(&u, &v))
        }
    }
}

/// Vorzeichenbehafteter Abstand des Punktes p von der Linie (links der Richtung positiv).
fn signed_distance(ctx: &Ctx, p: &V2, line: CurveId) -> Dual {
    let (a, d) = line_dir(ctx, line);
    cross(&d, &sub2(p, &a)) / norm_len(&d)
}

/// Die Enden einer Linie, eines Bogens oder eines Splines (sonst `None`).
pub(crate) fn curve_ends(kind: &CurveKind) -> Option<[PointId; 2]> {
    match kind {
        CurveKind::Line { a, b } => Some([*a, *b]),
        CurveKind::Arc { start, end, .. } => Some([*start, *end]),
        CurveKind::Spline { points, .. } | CurveKind::ControlSpline { points, .. } => {
            Some([*points.first()?, *points.last()?])
        }
        CurveKind::Conic { start, end, .. } => Some([*start, *end]),
        // Geschlossen: zweimal derselbe Punkt.
        CurveKind::FixedSpline { start, end, .. } => Some([*start, *end]),
        _ => None,
    }
}

/// Der Punkt der Linie `l2`, der kein Punkt der Linie `l1` ist (für Kollinear); `None`, wenn
/// beide Punkte geteilt sind oder eine der Kurven keine Linie ist.
pub(crate) fn collinear_point(sk: &Sketch, l1: CurveId, l2: CurveId) -> Option<PointId> {
    let (CurveKind::Line { a: a1, b: b1 }, CurveKind::Line { a: a2, b: b2 }) =
        (&sk.curves[l1].kind, &sk.curves[l2].kind)
    else {
        return None;
    };
    [*a2, *b2].into_iter().find(|p| p != a1 && p != b1)
}

/// Teilen die (geprüften) Linien `l1` und `l2` einen Punkt?
pub(crate) fn lines_share_point(sk: &Sketch, l1: CurveId, l2: CurveId) -> bool {
    let (CurveKind::Line { a: a1, b: b1 }, CurveKind::Line { a: a2, b: b2 }) =
        (&sk.curves[l1].kind, &sk.curves[l2].kind)
    else {
        return false;
    };
    [a2, b2].into_iter().any(|p| p == a1 || p == b1)
}

/// Richtung aus dem Ende `p` in die Linie, den Bogen oder den Spline `c` hinein; `None`, wenn `p`
/// (nicht mehr) ein Ende von `c` ist, etwa nach Stutzen, Brechen oder Rundung.
fn into_curve(ctx: &Ctx, c: CurveId, p: PointId) -> Option<V2> {
    let [first, last] = curve_ends(&ctx.sk.curves[c].kind)?;
    if p != first && p != last {
        return None;
    }
    Some(match &ctx.sk.curves[c].kind {
        CurveKind::Line { a, b } => {
            let (pa, pb) = (ctx.pt(*a), ctx.pt(*b));
            if p == *a {
                sub2(&pb, &pa)
            } else {
                sub2(&pa, &pb)
            }
        }
        // Bögen laufen gegen den Uhrzeigersinn vom Start zum Ende.
        CurveKind::Arc { center, start, .. } => {
            let r = sub2(&ctx.pt(p), &ctx.pt(*center));
            if p == *start {
                (-&r.1, r.0)
            } else {
                (r.1, -&r.0)
            }
        }
        CurveKind::Spline { points, .. } => {
            if p == first {
                param_curve(ctx, c, &Dual::constant(0.0)).1
            } else {
                let end = (points.len() - 1) as f64;
                let (_, d) = param_curve(ctx, c, &Dual::constant(end));
                (-&d.0, -&d.1)
            }
        }
        CurveKind::ControlSpline { knots, .. } => {
            if p == first {
                param_curve(ctx, c, &Dual::constant(*knots.first()?)).1
            } else {
                let (_, d) = param_curve(ctx, c, &Dual::constant(*knots.last()?));
                (-&d.0, -&d.1)
            }
        }
        CurveKind::Conic { .. } => {
            if p == first {
                param_curve(ctx, c, &Dual::constant(0.0)).1
            } else {
                let (_, d) = param_curve(ctx, c, &Dual::constant(1.0));
                (-&d.0, -&d.1)
            }
        }
        CurveKind::FixedSpline { curve, .. } => {
            // Geschlossen: `p` ist beides, es gilt der Anfang.
            let (lo, hi) = curve.domain();
            if p == first {
                param_curve(ctx, c, &Dual::constant(lo)).1
            } else {
                let (_, d) = param_curve(ctx, c, &Dual::constant(hi));
                (-&d.0, -&d.1)
            }
        }
        _ => return None,
    })
}

fn eval_kind(ctx: &Ctx, data: &ConstraintData) -> Vec<Dual> {
    // 3D-Geometrie rechnet in der 3D-Fassung (Skizzen 10); reine 2D-Geometrie genau wie bisher.
    if ctx.sk.constraint_is_3d(&data.kind)
        && let Some(rows) = crate::constraint3d::eval(ctx, data)
    {
        return rows;
    }
    eval_kind_2d(ctx, data)
}

/// Residuen in der Skizzenebene (die Fassung vor Skizzen 10).
fn eval_kind_2d(ctx: &Ctx, data: &ConstraintData) -> Vec<Dual> {
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
                // wie beim Bogen auf der ganzen Kurve
                CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
                    vec![ellipse_residual(ctx, *c, &pp)]
                }
                CurveKind::Spline { .. }
                | CurveKind::ControlSpline { .. }
                | CurveKind::Conic { .. }
                | CurveKind::FixedSpline { .. } => {
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
                unreachable!("checked in the guard")
            };
            let t = extra_var(ctx, data);
            let (q, dq) = param_curve(ctx, curve, &t);
            let (_, d) = line_dir(ctx, line);
            let den = norm_len(&dq) * norm_len(&d);
            vec![signed_distance(ctx, &q, line), cross(&dq, &d) / den]
        }
        // Am gemeinsamen Ende berührt die Tangente nur in zweiter Ordnung: Ihre Zeile fiele dort
        // mit `ArcRadius` zusammen (oder wäre null) und hielte die Richtung nicht. Darum zählt hier
        // die Richtung selbst, als Winkel: Radius senkrecht zur Linie, die Mitten auf einer Geraden
        // durchs Ende. Die umgekehrte Richtung (die Spitze) ist keine Nullstelle, sie liegt bei ±π.
        Tangent(c1, c2) if shared_end(sk, *c1, *c2).is_some() => {
            let Some(p) = shared_end(sk, *c1, *c2) else {
                unreachable!("checked in the guard")
            };
            let (sin, cos) = shared_end_parts(ctx, *c1, *c2, p);
            let s = if data.shared_sign < 0 { -1.0 } else { 1.0 };
            vec![sin.atan2(&(cos * s))]
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
        // An einem 2D-Punkt (z = 0, siehe `check_3d`) bleiben x und y.
        Fixed3(p, x, y, _) => {
            let pp = ctx.pt(*p);
            vec![&pp.0 - *x, &pp.1 - *y]
        }
        // An reiner 2D-Geometrie (nur aus der Datei denkbar): Die Linie fällt zu einem Punkt.
        AlongZ(l) => {
            let (a, b) = match l {
                LineOrPoints::Line(c) => {
                    let (a, d) = line_dir(ctx, *c);
                    let b = (&a.0 + &d.0, &a.1 + &d.1);
                    (a, b)
                }
                LineOrPoints::Points(a, b) => (ctx.pt(*a), ctx.pt(*b)),
            };
            vec![&a.0 - &b.0, &a.1 - &b.1]
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
                unreachable!("type was checked")
            };
            if sk.arc_is_3d(*c) {
                // 3D-Bogen (Skizzen 10): Abstände im Raum.
                let pc = ctx.pt3(center);
                return vec![
                    length3(&sub3(&ctx.pt3(end), &pc)) - length3(&sub3(&ctx.pt3(start), &pc)),
                ];
            }
            let pc = ctx.pt(center);
            vec![length(&sub2(&ctx.pt(end), &pc)) - length(&sub2(&ctx.pt(start), &pc))]
        }
        EllipticArcEnds(c) => {
            let CurveKind::EllipticArc { start, end, .. } = sk.curves[*c].kind else {
                unreachable!("type was checked")
            };
            if sk.arc_is_3d(*c) {
                return vec![
                    ellipse_residual3(ctx, *c, &ctx.pt3(start)),
                    ellipse_residual3(ctx, *c, &ctx.pt3(end)),
                ];
            }
            vec![
                ellipse_residual(ctx, *c, &ctx.pt(start)),
                ellipse_residual(ctx, *c, &ctx.pt(end)),
            ]
        }
        InPlane(c) => {
            let n = sk.curve_normal(*c);
            let off = |m: PointId, p: PointId| dot3c(&sub3(&ctx.pt3(p), &ctx.pt3(m)), n);
            match sk.curves[*c].kind {
                CurveKind::Arc { center, start, end } => vec![off(center, start), off(center, end)],
                CurveKind::Ellipse { ax, ay, az, .. } => {
                    vec![dot3c(&ellipse_axis3(ctx, ax, ay, az), n)]
                }
                CurveKind::EllipticArc {
                    center,
                    ax,
                    ay,
                    az,
                    start,
                    end,
                    ..
                } => vec![
                    dot3c(&ellipse_axis3(ctx, ax, ay, az), n),
                    off(center, start),
                    off(center, end),
                ],
                _ => unreachable!("type was checked"),
            }
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
        Collinear(l1, l2) => {
            let ((_, d1), (_, d2)) = (line_dir(ctx, *l1), line_dir(ctx, *l2));
            let den = norm_len(&d1) * norm_len(&d2);
            let Some(q) = collinear_point(sk, *l1, *l2) else {
                debug_assert!(false, "type was checked");
                return Vec::new();
            };
            let angle = cross(&d1, &d2) / den;
            // Mit gemeinsamem Ende ist der Abstand von `q` nur die Winkelzeile mal `|d2|`: Eine
            // zweite Zeile wäre stets abhängig und meldete die Abhängigkeit als überzählig.
            if lines_share_point(sk, *l1, *l2) {
                vec![angle]
            } else {
                vec![angle, signed_distance(ctx, &ctx.pt(q), *l1)]
            }
        }
        // Wie die Tangente am gemeinsamen Ende als Winkel: Die Spitze liegt bei ±π und ist keine
        // Nullstelle.
        // Ohne gültige Enden keine Zeile; `settle_constraints` und `satisfied` sorgen dafür, dass
        // ein solches Glatt entfällt.
        // Dazu die Krümmungszeile `κ₁ + κ₂ = 0` nach Spec C1: Die Richtungen in die Kurven laufen
        // entgegengesetzt.
        Smooth(c1, p1, c2, p2) => {
            let (Some(u), Some(v)) = (into_curve(ctx, *c1, *p1), into_curve(ctx, *c2, *p2)) else {
                return Vec::new();
            };
            let w = (-&v.0, -&v.1);
            let mut rows = vec![cross(&u, &w).atan2(&dot(&u, &w))];
            if let (Some(e1), Some(e2)) = (smooth_end(sk, *c1, *p1), smooth_end(sk, *c2, *p2))
                && smooth_has_curvature_row(e1, e2)
                && let (Some(k1), Some(k2)) = (
                    smooth_row_kappa(ctx, *c1, *p1, e1),
                    smooth_row_kappa(ctx, *c2, *p2, e2),
                )
            {
                rows.push(k1 + k2);
            }
            rows
        }
        DistanceX(a, b, v) | DistanceY(a, b, v) => {
            let d = sub2(&ctx.pt(*b), &ctx.pt(*a));
            let along = if matches!(data.kind, DistanceX(..)) {
                d.0
            } else {
                d.1
            };
            vec![f64::from(data.branch) * along - *v]
        }
        CenterlineDiameter(p, line, v) => {
            let pp = ctx.pt(*p);
            vec![2.0 * f64::from(data.branch) * signed_distance(ctx, &pp, *line) - *v]
        }
        SlotOverall(a, b, cap, v) => {
            let gap = length(&sub2(&ctx.pt(*a), &ctx.pt(*b)));
            vec![gap + 2.0 * circle_of(ctx, *cap).1 - *v]
        }
        // Keine Zeile: Die Exemplare hält der Löser selbst (`refresh_patterns`).
        PatternValue(..) => Vec::new(),
        // Ohne Krümmungsgriff keine Zeile; lebend lässt `check_handles` das nicht zu.
        SplineCurvatureHandle(c, at) => {
            let Some((tip, center, start, end)) = curvature_handle_points(sk, *c, *at) else {
                return Vec::new();
            };
            let (a, m) = (ctx.pt(*at), ctx.pt(center));
            let forward = sub2(&ctx.pt(tip), &a);
            let radial = sub2(&m, &a);
            // Mitte auf der Normalen, als Winkel von der Normalen auf ihrer Seite: links des Griffs,
            // wenn der Bogen am Stützpunkt beginnt, rechts, wenn er dort endet. Das nackte
            // `dot(radial, forward)` ist auch auf der anderen Seite null (B9: Die Seite wechselt nie
            // durch Lösen), und der Löser erfüllt es, indem er die Mitte auf die neue Normale
            // projiziert: Bei gedrehtem Griff schrumpft der Radius mit `|cos|`, bei 90° bis auf null.
            // Der Winkel dreht die Mitte stattdessen mit; die Gegenseite liegt bei ±π und ist keine
            // Nullstelle.
            let side = if start == *at { 1.0 } else { -1.0 };
            let off_normal = dot(&radial, &forward).atan2(&(side * cross(&forward, &radial)));
            // Bögen laufen gegen den Uhrzeigersinn vom Start zum Ende. Die Öffnung ist höchstens
            // 2,5 rad, der Sprung von `atan2` bei π ist also weit weg.
            let (s, e) = (sub2(&ctx.pt(start), &m), sub2(&ctx.pt(end), &m));
            let opening = cross(&s, &e).atan2(&dot(&s, &e));
            // Beide Zeilen sind Winkel. Mit einem Faktor `r` (Länge statt Winkel) wären sie bei
            // `r = 0` erfüllt, und der Löser fände dort ein Tal: Der Bogen fiele in sich zusammen.
            let ratio = norm_len(&forward) / norm_len(&radial);
            let want = if ratio.v < HANDLE_MAX_OPENING {
                ratio
            } else {
                Dual::constant(HANDLE_MAX_OPENING)
            };
            vec![off_normal, opening - want]
        }
    }
}

/// Spitze, Bogenmitte, Bogenstart und Bogenende des Krümmungsgriffs von `c` am Stützpunkt `at`.
fn curvature_handle_points(
    sk: &Sketch,
    c: CurveId,
    at: PointId,
) -> Option<(PointId, PointId, PointId, PointId)> {
    let CurveKind::Spline { handles, .. } = &sk.curves[c].kind else {
        return None;
    };
    let h = handles.iter().find(|h| h.at == at)?;
    let HandleCurvature::Arc { arc, center, .. } = h.curvature else {
        return None;
    };
    let CurveKind::Arc { start, end, .. } = sk.curves[arc].kind else {
        return None;
    };
    Some((h.tip, center, start, end))
}

/// Krümmung `κ = (C' × C'') / |C'|³` aus erster und zweiter Ableitung; 0 ohne Richtung.
fn kappa_of(first: &V2, second: &V2) -> Dual {
    let len2 = dot(first, first);
    if len2.v <= f64::MIN_POSITIVE {
        return Dual::constant(0.0);
    }
    cross(first, second) / (&len2 * &len2.sqrt())
}

/// Erste und zweite Ableitung am Anfang eines geklemmten B-Splines (Grad mindestens 1).
fn clamped_start_derivatives(degree: usize, knots: &[f64], pts: &[V2]) -> (V2, V2) {
    let zero = || (Dual::constant(0.0), Dual::constant(0.0));
    let quotient = |a: &V2, b: &V2, den: f64, f: f64| -> V2 {
        if den > 0.0 {
            let d = sub2(a, b);
            (&d.0 * (f / den), &d.1 * (f / den))
        } else {
            zero()
        }
    };
    let p = degree as f64;
    let first = quotient(&pts[1], &pts[0], knots[degree + 1] - knots[1], p);
    if degree < 2 || pts.len() < 3 {
        return (first, zero());
    }
    let next = quotient(&pts[2], &pts[1], knots[degree + 2] - knots[2], p);
    let second = quotient(&next, &first, knots[degree + 1] - knots[2], p - 1.0);
    (first, second)
}

/// Krümmung der Kurve `c` an ihrem Ende `p`, in Zahlen wie in Spec C2: Linie 0, Bogen `±1 / r`, Griff
/// `Arc` der Griffwert, alle übrigen aus ihrer zweiten Ableitung. Das Vorzeichen gilt in der Richtung
/// von `p` in die Kurve hinein: positiv bei Linksdrehung. `None`, wenn `p` kein Ende von `c` ist oder
/// die Kurve keine Enden hat.
pub(crate) fn end_curvature_dual(ctx: &Ctx, c: CurveId, p: PointId) -> Option<Dual> {
    let [first, last] = curve_ends(&ctx.sk.curves[c].kind)?;
    if p != first && p != last {
        return None;
    }
    // Vom hinteren Ende her läuft die Richtung `t` abwärts, das kehrt das Vorzeichen um.
    let along = |k: Dual| if p == first { k } else { -&k };
    Some(match &ctx.sk.curves[c].kind {
        CurveKind::Line { .. } => Dual::constant(0.0),
        // Gegen den Uhrzeigersinn vom Start aus, also links; vom Ende her rechts.
        CurveKind::Arc { center, start, .. } => {
            let r = norm_len(&sub2(&ctx.pt(*start), &ctx.pt(*center)));
            along(Dual::constant(1.0) / &r)
        }
        // Natürliches Ende.
        CurveKind::Spline { handles, .. } if handles.is_empty() => Dual::constant(0.0),
        CurveKind::Spline { points, handles } => {
            if let Some(h) = handles.iter().find(|h| h.at == p)
                && let HandleCurvature::Arc { center, .. } = h.curvature
            {
                let (at, tip, m) = (ctx.pt(p), ctx.pt(h.tip), ctx.pt(center));
                return Some(along(handle_kappa(&at, &tip, &m)));
            }
            let nodes = nodes_of(points, handles, |q| ctx.pt(q));
            let i = if p == first { 0 } else { points.len() - 1 };
            along(kappa_of(&nodes.first[i], &nodes.second[i]))
        }
        CurveKind::ControlSpline {
            points,
            degree,
            knots,
        } => {
            // Vom hinteren Ende aus ist es der Anfang der umgedrehten Kurve; das Vorzeichen gilt
            // damit schon in die Kurve hinein.
            let mut pts: Vec<V2> = points.iter().map(|&q| ctx.pt(q)).collect();
            let mut u = knots.clone();
            if p != first {
                pts.reverse();
                u.reverse();
                u.iter_mut().for_each(|v| *v = -*v);
            }
            let (d1, d2) = clamped_start_derivatives(usize::from(*degree), &u, &pts);
            kappa_of(&d1, &d2)
        }
        CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        } => {
            // Rationale Bézierkurve Grad 2 mit den Gewichten 1, w, 1 an ihrem Anfang:
            // `κ = ((B − A) × (E − B)) / (2·w²·|B − A|³)`.
            let (s, e) = if p == *start {
                (*start, *end)
            } else {
                (*end, *start)
            };
            let (a, b, f) = (ctx.pt(s), ctx.pt(*apex), ctx.pt(e));
            let (u, v) = (sub2(&b, &a), sub2(&f, &b));
            let w = conic_weight(*rho);
            kappa_of(&u, &v) * (0.5 / (w * w))
        }
        CurveKind::FixedSpline { curve, .. } => {
            let (lo, hi) = curve.domain();
            let d = curve.derivs(if p == first { lo } else { hi }, 2);
            let (d1, d2) = (d[1].truncate(), d[2].truncate());
            let speed = d1.length();
            along(Dual::constant(if speed > 0.0 {
                d1.perp_dot(d2) / (speed * speed * speed)
            } else {
                0.0
            }))
        }
        // Diese Arten haben keine Enden; `curve_ends` oben hat sie schon abgewiesen.
        CurveKind::Circle { .. } | CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
            return None;
        }
    })
}

/// Woher die Krümmung am Ende einer Glatt-Seite kommt (Spec C1). Hier und in den `match` darunter steht
/// die ganze Regel, wer mit wem geht und was der Befehl tut; eine neue Kurvenart scheitert in
/// [`smooth_end`], [`smooth_step`], [`smooth_leader`] und [`smooth_partner_kind`] beim Übersetzen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SmoothEnd {
    Line,
    /// Spline durch Stützpunkte, an diesem Ende ohne Griff: Krümmung von selbst 0.
    Natural,
    /// Griff `Straight`: Krümmung von selbst 0.
    Straight,
    /// Griff `Form`: Krümmung aus der Kurve, nicht steuerbar; das Glatt gilt dort als G1.
    Form,
    /// Griff `Arc`.
    ArcHandle,
    Arc,
    /// Die Kontrollpunkte passen sich an, auch auf Krümmung 0.
    ControlSpline,
    Conic,
    /// Mit eigener Krümmung, die sich nie ändert (C5).
    FixedSpline,
}

/// Die Art der Seite `c` am Ende `p`; `None` ohne Enden oder wenn `p` keines ist.
pub(crate) fn smooth_end(sk: &Sketch, c: CurveId, p: PointId) -> Option<SmoothEnd> {
    let kind = &sk.curves[c].kind;
    if !curve_ends(kind)?.contains(&p) {
        return None;
    }
    Some(match kind {
        CurveKind::Line { .. } => SmoothEnd::Line,
        CurveKind::Arc { .. } => SmoothEnd::Arc,
        CurveKind::ControlSpline { .. } => SmoothEnd::ControlSpline,
        CurveKind::Conic { .. } => SmoothEnd::Conic,
        CurveKind::FixedSpline { .. } => SmoothEnd::FixedSpline,
        CurveKind::Spline { handles, .. } => match handles.iter().find(|h| h.at == p) {
            None => SmoothEnd::Natural,
            Some(h) => match h.curvature {
                HandleCurvature::Form => SmoothEnd::Form,
                HandleCurvature::Straight => SmoothEnd::Straight,
                HandleCurvature::Arc { .. } => SmoothEnd::ArcHandle,
            },
        },
        CurveKind::Circle { .. } | CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
            return None;
        }
    })
}

/// Gibt es die Krümmungszeile (C1)? Ein Spline-Ende ohne Griff und ein Griff `Form` haben nie eine:
/// Gegen Krümmung 0 ist die Fuge beim natürlichen Ende schon G2, sonst ist es der G1-Fall aus C4. Zwei
/// Seiten mit Krümmung 0 brauchen auch keine.
fn smooth_has_curvature_row(a: SmoothEnd, b: SmoothEnd) -> bool {
    let free = |e: SmoothEnd| matches!(e, SmoothEnd::Natural | SmoothEnd::Form);
    let zero = |e: SmoothEnd| matches!(e, SmoothEnd::Line | SmoothEnd::Straight);
    !(free(a) || free(b) || (zero(a) && zero(b)))
}

/// Krümmung einer Seite in der Krümmungszeile; `None` für Seiten, die nie eine Zeile haben.
fn smooth_row_kappa(ctx: &Ctx, c: CurveId, p: PointId, end: SmoothEnd) -> Option<Dual> {
    match end {
        SmoothEnd::Line | SmoothEnd::Straight => Some(Dual::constant(0.0)),
        SmoothEnd::ArcHandle
        | SmoothEnd::Arc
        | SmoothEnd::ControlSpline
        | SmoothEnd::Conic
        | SmoothEnd::FixedSpline => end_curvature_dual(ctx, c, p),
        SmoothEnd::Natural | SmoothEnd::Form => None,
    }
}

/// Was der Befehl Glatt an einer Seite tut (Spec C3), siehe [`smooth_step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SmoothStep {
    /// Nichts.
    Keep,
    /// Griffe an, Tangente und Krümmung vom Partner (C3, erster Fall).
    Follow,
    /// Griff `Straight`, ein Krümmungsbogen entfällt (C3, zweiter Fall).
    Straighten,
    /// Gleicher Rang: Beide Seiten bekommen den Mittelwert aus Tangente und Krümmung.
    Mean,
}

/// Der Schritt für die Seite `me` gegen `other`. Er hängt nicht von der Wahlreihenfolge ab.
pub(crate) fn smooth_step(me: SmoothEnd, other: SmoothEnd) -> SmoothStep {
    use SmoothEnd::*;
    use SmoothStep::*;
    match (me, other) {
        // Nur ein Spline durch Stützpunkte hat Griffe. Der Kontrollpunkt-Spline passt sich über die
        // Krümmungszeile an, der feste Spline nie.
        (Line | Arc | ControlSpline | Conic | FixedSpline, _) => Keep,
        (Natural, Arc | ArcHandle | ControlSpline | Conic | FixedSpline) => Follow,
        (Natural, Line | Natural | Straight | Form) => Keep,
        // Gegen einen Bogen oder Kegelschnitt ginge Krümmung 0 nie; ein Kontrollpunkt-Spline kann sie.
        (Straight, Arc | ArcHandle | Conic | FixedSpline) => Follow,
        (Straight, Line | Natural | Straight | Form | ControlSpline) => Keep,
        (Form, Arc | ArcHandle | ControlSpline | Conic | FixedSpline) => Follow,
        (Form, Line | Natural | Straight) => Straighten,
        (Form, Form) | (ArcHandle, ArcHandle) => Mean,
        (ArcHandle, Line) => Straighten,
        (ArcHandle, Arc | ControlSpline | Conic | FixedSpline) => Follow,
        (ArcHandle, Natural | Straight | Form) => Keep,
    }
}

/// Kann diese Art die eine Seite sein, die jedes Glatt braucht (C5)?
fn smooth_leader(k: &CurveKind) -> bool {
    match k {
        CurveKind::Spline { .. }
        | CurveKind::ControlSpline { .. }
        | CurveKind::FixedSpline { .. } => true,
        CurveKind::Line { .. }
        | CurveKind::Circle { .. }
        | CurveKind::Arc { .. }
        | CurveKind::Ellipse { .. }
        | CurveKind::EllipticArc { .. }
        | CurveKind::Conic { .. } => false,
    }
}

/// Kann diese Art überhaupt an einem Glatt teilhaben (C5)? Ellipsenbogen bleibt abgelehnt.
fn smooth_partner_kind(k: &CurveKind) -> bool {
    match k {
        CurveKind::Line { .. }
        | CurveKind::Arc { .. }
        | CurveKind::Spline { .. }
        | CurveKind::ControlSpline { .. }
        | CurveKind::Conic { .. }
        | CurveKind::FixedSpline { .. } => true,
        CurveKind::Circle { .. } | CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
            false
        }
    }
}

/// Radius und Durchmesser umgeschaltet: `Radius(c, v) ↔ Diameter(c, 2v)`,
/// `DistancePointLine(p, L, v) ↔ CenterlineDiameter(p, L, 2v)`. Ob die Linie eine Mittellinie
/// ist, prüft der Aufrufer. Die Faktoren sind exakt, zweimal umgeschaltet ist der Wert bitgleich.
/// `None` für andere Arten.
pub(crate) fn toggled_kind(kind: &ConstraintKind) -> Option<ConstraintKind> {
    use ConstraintKind::*;
    Some(match *kind {
        Radius(c, v) => Diameter(c, 2.0 * v),
        Diameter(c, v) => Radius(c, 0.5 * v),
        DistancePointLine(p, l, v) => CenterlineDiameter(p, l, 2.0 * v),
        CenterlineDiameter(p, l, v) => DistancePointLine(p, l, 0.5 * v),
        _ => return None,
    })
}

/// Endliche Länge oder Koordinate mit Betrag höchstens `MAX_COORD`.
fn finite(v: f64) -> Result<(), SketchError> {
    crate::model::finite(v, || tr!("sketch-err-what-value"))
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
        Some(self.evaluate_data(&self.constraints[id], x))
    }

    /// Nur die lokalen Variablen einer lebenden Abhängigkeit an `x`, genau wie
    /// `evaluate(id, x).locals`, aber ohne die Rechnung mit Ableitungen: Der zweite Durchlauf
    /// von [`evaluate_data`](Self::evaluate_data) meldet keine neue Variable.
    pub(crate) fn constraint_locals(&self, id: ConstraintId, x: &[f64]) -> Option<Vec<usize>> {
        if id.index() >= self.constraints.len()
            || !self.constraints[id].alive
            || x.len() < self.vars.len()
        {
            return None;
        }
        let first = Ctx::collecting(self, x);
        let _ = eval_kind(&first, &self.constraints[id]);
        Some(first.locals.into_inner())
    }

    /// Wertet `data` an `x` aus (auch eine noch nicht angelegte Abhängigkeit); `x` muss alle
    /// Variablen von `data` enthalten.
    fn evaluate_data(&self, data: &ConstraintData, x: &[f64]) -> Evaluation {
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
        Evaluation {
            locals: second.locals.into_inner(),
            residuals,
        }
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

    pub(crate) fn push_internal(&mut self, kind: ConstraintKind) -> ConstraintId {
        self.touch_structure();
        self.constraints.push(ConstraintData {
            kind,
            branch: 0,
            shared_sign: 0,
            driven: false,
            enabled: true,
            internal: true,
            extra: None,
            alive: true,
        })
    }

    fn checked_constraint(&self, id: ConstraintId) -> Result<(), SketchError> {
        if self.constraint(id).is_some() {
            Ok(())
        } else {
            Err(SketchError::UnknownObject)
        }
    }

    /// Wird die Abhängigkeit dadurch wieder erzwungen, gilt die Prüfung auf eine Schleife
    /// zwischen Vorlage und eigenem Exemplar wie bei [`Sketch::add_constraint`].
    pub fn set_driven(&mut self, id: ConstraintId, driven: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        if driven {
            self.check_not_pattern_value(id)?;
            self.check_not_fixed_spline_end(id)?;
        } else if self.constraints[id].driven && self.constraints[id].enabled {
            self.check_activation_cycle(id)?;
            self.check_not_instance_only(id)?;
        }
        self.constraints[id].driven = driven;
        self.touch_structure();
        Ok(())
    }

    /// Beim Einschalten gilt die Prüfung auf eine Schleife wie bei [`Sketch::add_constraint`].
    pub fn set_enabled(&mut self, id: ConstraintId, enabled: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        if enabled && !self.constraints[id].enabled && !self.constraints[id].driven {
            self.check_activation_cycle(id)?;
            self.check_not_instance_only(id)?;
        }
        if !enabled {
            self.check_not_fixed_spline_end(id)?;
        }
        self.constraints[id].enabled = enabled;
        self.touch_structure();
        Ok(())
    }

    /// Ändert den Zahlenwert eines Maßes. Die gewählte Lage (`branch`) bleibt erhalten. Ein
    /// Musterwert muss eine gültige Lage ergeben; eine neue Anzahl gleicht die Exemplare an
    /// (`sync_pattern`): Bleibende behalten ihre Ids, wegfallende nehmen ihre Abhängigkeiten mit.
    /// Bei einem Fehler bleibt die Skizze unverändert.
    pub fn set_constraint_value(&mut self, id: ConstraintId, v: f64) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        let kind = self.constraints[id]
            .kind
            .with_value(v)
            .ok_or_else(|| SketchError::WrongType(tr!("sketch-not-dimension")))?;
        self.check_refs(&kind)?;
        // Ein treibendes Maß nur an Exemplaren (aus einer alten Datei) bekäme einen Wert ohne
        // Wirkung: abgelehnt (Schlusskorrektur I1). Anzeigend darf es den Messwert speichern.
        let d = &self.constraints[id];
        if d.enabled && !d.driven {
            self.check_not_instance_only(id)?;
        }
        if let ConstraintKind::PatternValue(m, slot, v) = kind {
            let plan = self
                .pattern_plan(m)
                .ok_or(SketchError::UnknownObject)?
                .with_value(slot, v);
            self.pattern_lattice(&plan).map_err(PatternProblem::error)?;
            return self.atomically(|s| {
                // Eine neue Anzahl kann Exemplare wegnehmen und so ein Muster in eine Schleife
                // bringen (siehe `refuse_new_cycles`); ein Abstand oder Winkel nicht.
                let before = if slot.is_count() {
                    s.cyclic_patterns()
                } else {
                    Vec::new()
                };
                s.constraints[id].kind = kind;
                s.touch_structure();
                if slot.is_count() {
                    s.sync_pattern(m)?;
                    s.refuse_new_cycles(&before)?;
                }
                Ok(())
            });
        }
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
        if self.constraint_is_3d(kind)
            && let Some(v) = self.measure3(kind)
        {
            return v.is_finite().then_some(v);
        }
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
            DistanceX(a, b, _) => (ctx.pt(*b).0.v - ctx.pt(*a).0.v).abs(),
            DistanceY(a, b, _) => (ctx.pt(*b).1.v - ctx.pt(*a).1.v).abs(),
            CenterlineDiameter(p, l, _) => 2.0 * signed_distance(&ctx, &ctx.pt(*p), *l).v.abs(),
            SlotOverall(a, b, cap, _) => {
                length(&sub2(&ctx.pt(*a), &ctx.pt(*b))).v + 2.0 * circle_of(&ctx, *cap).1.v
            }
            // Der gespeicherte Wert des Musters ist sein Wert.
            PatternValue(m, slot, _) => self.pattern_value(*m, *slot)?,
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    /// Prüft Ids, Objekttypen und Zahlenwerte einer Abhängigkeit.
    pub(crate) fn check_refs(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        self.check_structure(kind)?;
        self.check_constraint_value(kind)
    }

    /// Prüft Ids und Objekttypen (ohne die Zahlenwerte der Maße).
    pub(crate) fn check_structure(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        for p in kind.points() {
            self.check_point(p)?;
        }
        for c in kind.curves() {
            self.check_curve(c)?;
        }
        self.check_types(kind)?;
        // Nicht in `check_types`: Ein gelöschtes Glatt behält nach Stutzen oder Rundung einen
        // Punkt, der kein Ende mehr ist, und die Datei muss trotzdem laden.
        if self.smooth_ends_hold(kind) {
            Ok(())
        } else {
            Err(SketchError::WrongType(tr!("sketch-err-smooth-end")))
        }
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
            Horizontal(LineOrPoints::Line(c))
            | Vertical(LineOrPoints::Line(c))
            | AlongZ(LineOrPoints::Line(c))
                if !is_line(*c) =>
            {
                wrong(tr!("sketch-err-only-lines"))
            }
            Horizontal(LineOrPoints::Points(a, b))
            | Vertical(LineOrPoints::Points(a, b))
            | AlongZ(LineOrPoints::Points(a, b))
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
            EllipticArcEnds(c)
                if !matches!(self.curves[*c].kind, CurveKind::EllipticArc { .. }) =>
            {
                wrong(tr!("sketch-err-only-elliptic-arcs"))
            }
            // Die Normale selbst prüft das Laden (`check_curve_normal`); ein gelöschter Eintrag
            // darf an einer Kurve stehen, die wieder in der Skizzenebene liegt.
            InPlane(c)
                if !matches!(
                    self.curves[*c].kind,
                    CurveKind::Arc { .. }
                        | CurveKind::Ellipse { .. }
                        | CurveKind::EllipticArc { .. }
                ) =>
            {
                wrong(tr!("sketch-err-normal-kind"))
            }
            EllipseMajor(c, _) | EllipseMinor(c, _) | EllipseAngle(c, _)
                if !is_ellipse_kind(&self.curves[*c].kind) =>
            {
                wrong(tr!("sketch-err-only-ellipses"))
            }
            Collinear(a, b) if !(is_line(*a) && is_line(*b)) => wrong(tr!("sketch-err-only-lines")),
            Collinear(a, b) if a == b || collinear_point(self, *a, *b).is_none() => {
                degenerate(tr!("sketch-err-two-objects"))
            }
            Smooth(a, _, b, _) if a == b => degenerate(tr!("sketch-err-two-objects")),
            Smooth(a, _, b, _) => {
                if self.smooth_fixed_against_line(*a, *b) {
                    wrong(tr!("sketch-smooth-fixed-line"))
                } else if self.smooth_kinds_fit(*a, *b) {
                    Ok(())
                } else {
                    wrong(tr!("sketch-smooth-kinds"))
                }
            }
            DistanceX(a, b, _) | DistanceY(a, b, _) | SlotOverall(a, b, ..) if a == b => {
                degenerate(tr!("sketch-err-two-points"))
            }
            CenterlineDiameter(_, l, _) if !is_line(*l) => wrong(tr!("sketch-err-only-lines")),
            SlotOverall(_, _, cap, _) if !is_round(*cap) => wrong(tr!("sketch-err-only-circles")),
            // Dass an `at` wirklich ein Krümmungsgriff sitzt, prüft `check_handles`: Ein gelöschter
            // Eintrag behält die Zeile, auch wenn der Griff längst weg ist.
            SplineCurvatureHandle(c, _)
                if !matches!(self.curves[*c].kind, CurveKind::Spline { .. }) =>
            {
                wrong(tr!("sketch-err-handle-kind"))
            }
            PatternValue(m, slot, _) => match self.patterns.get(*m) {
                None => Err(SketchError::UnknownObject),
                Some(d) if !slot.fits(&d.kind) => wrong(tr!("sketch-pattern-slot")),
                Some(_) => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// Prüft die Zahlenwerte eines Maßes oder einer festen Lage, ohne Ids und Objekttypen zu
    /// prüfen. Dieselbe Regel gilt für [`Sketch::add_constraint`],
    /// [`Sketch::set_constraint_value`] und die Eingabe in der Oberfläche:
    /// - alle Werte endlich, Längen und Koordinaten betragsmäßig höchstens
    ///   [`MAX_COORD`](crate::model::MAX_COORD);
    /// - Abstand zweier Punkte (auch waagerecht und senkrecht), Radius, Durchmesser (auch zur
    ///   Mittellinie), Gesamtlänge des Langlochs und Halbachsen größer als `tol.linear`
    ///   (ein Abstand null ließe eine Linie zu einem Punkt zusammenfallen; dafür gibt es
    ///   „deckungsgleich“);
    /// - Abstand Punkt–Linie nicht negativ (null heißt: Punkt auf der Linie);
    /// - die kleine Halbachse nicht länger als ein erzwungenes Gegenstück (siehe
    ///   `check_axis_pair`); Winkel beliebig.
    pub fn check_constraint_value(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        use ConstraintKind::*;
        match kind {
            Distance(_, _, v)
            | Radius(_, v)
            | Diameter(_, v)
            | DistanceX(_, _, v)
            | DistanceY(_, _, v)
            | CenterlineDiameter(_, _, v)
            | SlotOverall(_, _, _, v) => self.check_positive(*v),
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
            PatternValue(_, slot, v) => self.check_pattern_value(*slot, *v),
            Fixed(_, x, y) => {
                finite(*x)?;
                finite(*y)
            }
            Fixed3(_, x, y, z) => {
                finite(*x)?;
                finite(*y)?;
                finite(*z)
            }
            _ => Ok(()),
        }
    }

    /// Musterwerte (Entscheidung 3): Anzahl ganzzahlig 1 bis [`PATTERN_MAX_INSTANCES`]; Abstand
    /// endlich, Betrag über `tol.linear` und höchstens `MAX_COORD` (das Vorzeichen kehrt die
    /// Richtung um); Winkel endlich, nicht null, Betrag höchstens 2π.
    fn check_pattern_value(&self, slot: PatternSlot, v: f64) -> Result<(), SketchError> {
        if slot.is_count() {
            if v.is_finite()
                && v.fract() == 0.0
                && (1.0..=PATTERN_MAX_INSTANCES as f64).contains(&v)
            {
                Ok(())
            } else {
                Err(SketchError::InvalidInput(tr!(
                    "sketch-pattern-count-range",
                    max = PATTERN_MAX_INSTANCES.to_string()
                )))
            }
        } else if slot.is_spacing() {
            finite(v)?;
            self.check_positive(v.abs())
        } else {
            finite_angle(v)?;
            let tol = self.tolerance().angular;
            if v.abs() > tol && v.abs() <= planora_math::TAU + tol {
                Ok(())
            } else {
                Err(SketchError::Degenerate(tr!("sketch-pattern-angle-range")))
            }
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
        let at = |p: PointId| DVec2::new(x[self.points[p].x], x[self.points[p].y]);
        let tol = self.tolerance().linear;
        data.kind
            .curves()
            .iter()
            .find_map(|&c| match &self.curves[c].kind {
                CurveKind::Spline { points, .. } => {
                    let n = points.len();
                    // Spielraum je Ende: `tol.linear` geteilt durch die Sehnenlänge des
                    // End-Segments, also etwa `tol.linear` Weglänge auf der Fortsetzung, begrenzt
                    // auf [MIN_SPLINE_T_TOL, MAX_SPLINE_T_TOL].
                    let slack = |a: PointId, b: PointId| {
                        (tol / at(a).distance(at(b))).clamp(MIN_SPLINE_T_TOL, MAX_SPLINE_T_TOL)
                    };
                    Some(SplineParameter {
                        var,
                        lo: 0.0,
                        hi: (n - 1) as f64,
                        slack_start: slack(points[0], points[1]),
                        slack_end: slack(points[n - 2], points[n - 1]),
                    })
                }
                CurveKind::ControlSpline {
                    points,
                    degree,
                    knots,
                } => {
                    let (p, n, k) = (usize::from(*degree), points.len(), knots.len());
                    // Wie oben, mit der Geschwindigkeit am Ende `p / reach · Sehne`. Zusammen-
                    // gefallene Punkte (Sehne null) ergeben die Obergrenze; sie wächst mit der
                    // Breite des End-Stücks, damit auch schmale Bereiche nur wenig überragt werden.
                    let slack = |a: PointId, b: PointId, reach: f64, piece: f64| {
                        (tol * reach / (p as f64 * at(a).distance(at(b)))).clamp(
                            MIN_SPLINE_T_TOL,
                            (MAX_SPLINE_T_TOL * piece).max(MIN_SPLINE_T_TOL),
                        )
                    };
                    Some(SplineParameter {
                        var,
                        lo: knots[0],
                        hi: knots[k - 1],
                        slack_start: slack(
                            points[0],
                            points[1],
                            knots[p + 1] - knots[1],
                            knots[p + 1] - knots[p],
                        ),
                        slack_end: slack(
                            points[n - 2],
                            points[n - 1],
                            knots[k - 2] - knots[k - p - 2],
                            knots[k - p - 1] - knots[k - p - 2],
                        ),
                    })
                }
                CurveKind::Conic {
                    start,
                    end,
                    apex,
                    rho,
                } => {
                    // Wie oben, mit der Geschwindigkeit am Ende `2w · Schenkel`.
                    let w = conic_weight(*rho);
                    let slack = |a: PointId, b: PointId| {
                        (tol / (2.0 * w * at(a).distance(at(b))))
                            .clamp(MIN_SPLINE_T_TOL, MAX_SPLINE_T_TOL)
                    };
                    Some(SplineParameter {
                        var,
                        lo: 0.0,
                        hi: 1.0,
                        slack_start: slack(*start, *apex),
                        slack_end: slack(*apex, *end),
                    })
                }
                CurveKind::FixedSpline { curve, .. } => {
                    // Wie oben, mit der Geschwindigkeit der Kurve an ihren Enden.
                    let (lo, hi) = curve.domain();
                    let (p, k) = (curve.degree(), curve.knots().knots());
                    let n = k.len();
                    let slack = |t: f64, piece: f64| {
                        (tol / curve.derivs(t, 1)[1].length()).clamp(
                            MIN_SPLINE_T_TOL,
                            (MAX_SPLINE_T_TOL * piece).max(MIN_SPLINE_T_TOL),
                        )
                    };
                    Some(SplineParameter {
                        var,
                        lo,
                        hi,
                        slack_start: slack(lo, k[p + 1] - k[p]),
                        slack_end: slack(hi, k[n - p - 1] - k[n - p - 2]),
                    })
                }
                _ => None,
            })
    }

    /// Vorzeichen der Tangente `kind` an einem gemeinsamen Endpunkt, aus der aktuellen Geometrie:
    /// die nächste Nullstelle. `0`, wenn `kind` keine solche Tangente ist.
    pub(crate) fn shared_end_sign(&self, kind: &ConstraintKind) -> i8 {
        let ConstraintKind::Tangent(a, b) = *kind else {
            return 0;
        };
        let Some(p) = shared_end(self, a, b) else {
            return 0;
        };
        if self.constraint_is_3d(kind) {
            return self.shared_end_sign3(a, b, p);
        }
        let ctx = Ctx::collecting(self, &self.vars);
        if shared_end_parts(&ctx, a, b, p).1.v < 0.0 {
            -1
        } else {
            1
        }
    }

    /// Bestimmt die Vorzeichen aller Tangenten neu, die an `curve` hängen (`None`: aller).
    pub(crate) fn refresh_shared_signs(&mut self, curve: Option<CurveId>) {
        let found: Vec<(ConstraintId, i8)> = self
            .constraints
            .iter()
            .filter(|(_, c)| {
                c.alive
                    && matches!(c.kind, ConstraintKind::Tangent(a, b)
                        if curve.is_none_or(|x| x == a || x == b))
            })
            .map(|(id, c)| (id, self.shared_end_sign(&c.kind)))
            .collect();
        for (id, sign) in found {
            self.constraints[id].shared_sign = sign;
        }
    }

    /// Lage bei mehrdeutigen Abhängigkeiten, bestimmt aus der aktuellen Geometrie.
    fn initial_branch(&self, kind: &ConstraintKind) -> i8 {
        use ConstraintKind::*;
        if self.constraint_is_3d(kind)
            && let Some(b) = self.initial_branch3(kind)
        {
            return b;
        }
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
            DistancePointLine(p, l, _) | CenterlineDiameter(p, l, _) => {
                sign(signed_distance(&ctx, &ctx.pt(*p), *l).v)
            }
            DistanceX(a, b, _) => sign(self.pos(*b).x - self.pos(*a).x),
            DistanceY(a, b, _) => sign(self.pos(*b).y - self.pos(*a).y),
            _ => 0,
        }
    }

    /// Abgetastete Form einer Ellipse, der ganzen Ellipse eines Ellipsenbogens, eines Splines oder
    /// einer Freiformkurve (sonst `None`).
    fn sampled(&self, c: CurveId) -> Option<Sampled> {
        match &self.curves[c].kind {
            CurveKind::Ellipse {
                center,
                ax,
                ay,
                minor,
                ..
            }
            | CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                ..
            } => Some(Sampled::ellipse(
                self.pos(*center),
                DVec2::new(self.vars[*ax], self.vars[*ay]),
                self.vars[*minor],
            )),
            CurveKind::Spline { points, handles } => Some(self.spline_sampled(points, handles)),
            CurveKind::ControlSpline { .. }
            | CurveKind::Conic { .. }
            | CurveKind::FixedSpline { .. } => Some(Sampled::Nurbs(self.nurbs_of(c)?)),
            _ => None,
        }
    }

    /// Abweichung (rad, ohne Vorzeichen) der Tangente `id` an einem gemeinsamen Endpunkt an den
    /// Werten `x`; `None`, wenn `id` keine solche Tangente ist.
    pub(crate) fn shared_end_tangent_drift(&self, id: ConstraintId, x: &[f64]) -> Option<f64> {
        let ConstraintKind::Tangent(a, b) = self.constraints[id].kind else {
            return None;
        };
        shared_end(self, a, b)?;
        // Die Zeilen sind die Abweichung selbst (in 3D zwei Winkel; NaN bleibt NaN).
        let ev = self.evaluate(id, x)?;
        ev.residuals.first()?;
        Some(
            ev.residuals
                .iter()
                .map(|r| r.v.abs())
                .fold(0.0, |m: f64, v| {
                    if m.is_nan() || v.is_nan() {
                        f64::NAN
                    } else {
                        m.max(v)
                    }
                }),
        )
    }

    /// Ist die lebende Abhängigkeit an den aktuellen Werten ohne Bewegung erfüllt? Alle Residuen
    /// höchstens `factor · tol.linear`, Winkelzeilen (parallel, rechtwinklig, Winkel,
    /// Ellipsenwinkel, Richtung bei Tangente Linie–Ellipse/Spline, Kollinear und Glatt) höchstens
    /// `factor · tol.angular`; eine Tangente am gemeinsamen Endpunkt innerhalb von
    /// [`SHARED_END_TANGENT_DRIFT`].
    pub(crate) fn satisfied(&self, id: ConstraintId, factor: f64) -> bool {
        use ConstraintKind::*;
        if !self.smooth_ends_hold(&self.constraints[id].kind) {
            return false;
        }
        if self
            .shared_end_tangent_drift(id, &self.vars)
            .is_some_and(|d| d <= SHARED_END_TANGENT_DRIFT)
        {
            return true;
        }
        let Some(ev) = self.evaluate(id, &self.vars) else {
            return false;
        };
        let kind = &self.constraints[id].kind;
        let is_3d = self.constraint_is_3d(kind);
        let angular_row = |row: usize| match kind {
            _ if is_3d => crate::constraint3d::angular_row(self, kind, row),
            Parallel(..) | Perpendicular(..) | Angle(..) | EllipseAngle(..) => true,
            Collinear(..) | Smooth(..) => row == 0,
            SplineCurvatureHandle(..) => true,
            Tangent(a, b) => row == 1 && line_and_param(self, *a, *b).is_some(),
            _ => false,
        };
        let tol = self.tolerance();
        ev.residuals.iter().enumerate().all(|(row, r)| {
            let limit = if angular_row(row) {
                tol.angular
            } else {
                tol.linear
            };
            r.v.abs() <= factor * limit
        })
    }

    /// Braucht die (geprüfte) Abhängigkeit eine Zusatzvariable? Dieselbe Regel wie in
    /// `extra_start`: Punkt auf Spline, Tangente zwischen Linie und Ellipse/Spline.
    pub(crate) fn needs_extra(&self, kind: &ConstraintKind) -> bool {
        match kind {
            ConstraintKind::PointOnCurve(_, c) => on_curve_needs_parameter(&self.curves[*c].kind),
            ConstraintKind::Tangent(a, b) => line_and_param(self, *a, *b).is_some(),
            _ => false,
        }
    }

    /// Startwert der Zusatzvariablen einer (geprüften) parametrischen Abhängigkeit: der
    /// nächstgelegene Kurvenparameter. `None`, wenn die Abhängigkeit keine braucht.
    pub(crate) fn extra_start(&self, kind: &ConstraintKind) -> Option<f64> {
        if self.constraint_is_3d(kind) && self.needs_extra(kind) {
            return self.extra_start3(kind);
        }
        match kind {
            ConstraintKind::PointOnCurve(p, c)
                if on_curve_needs_parameter(&self.curves[*c].kind) =>
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

    /// „Fest“ als Schalter (wie Fixieren/Lösen in Fusion): Hat jeder Punkt in `points` schon ein
    /// `Fixed`, werden diese gelöst, sonst bekommen die übrigen eins an ihrer jetzigen Lage.
    /// Doppelte Punkte zählen einmal. Das `Fixed` am Ende eines festen Splines bleibt und zählt
    /// nicht mit; was die Skizze sonst ablehnt, wird übersprungen. Die Zahl nennt, wie viele
    /// Abhängigkeiten angelegt oder entfernt wurden. Gelöst wird hier nicht.
    pub fn toggle_fixed(&mut self, points: &[PointId]) -> FixToggle {
        let mut pts: Vec<PointId> = Vec::new();
        for &p in points {
            if self.point_xy(p).is_ok() && !pts.contains(&p) {
                pts.push(p);
            }
        }
        let fixed_of = |s: &Sketch, p: PointId| -> Vec<ConstraintId> {
            s.constraints()
                .filter(|(_, c)| {
                    matches!(c.kind, ConstraintKind::Fixed(q, ..) | ConstraintKind::Fixed3(q, ..)
                        if q == p)
                })
                .map(|(i, _)| i)
                .collect()
        };
        if !pts.is_empty() && pts.iter().all(|&p| !fixed_of(self, p).is_empty()) {
            let mut n = 0;
            for &p in &pts {
                for id in fixed_of(self, p) {
                    if self.remove_constraint(id).is_ok() {
                        n += 1;
                    }
                }
            }
            return FixToggle::Released(n);
        }
        let mut n = 0;
        for &p in &pts {
            if !fixed_of(self, p).is_empty() {
                continue;
            }
            let Ok(at) = self.point_xyz(p) else { continue };
            // Am 3D-Punkt ist auch z fest (Ruling R2).
            let kind = if self.is_3d_point(p) {
                ConstraintKind::Fixed3(p, at.x, at.y, at.z)
            } else {
                ConstraintKind::Fixed(p, at.x, at.y)
            };
            if self.add_constraint(kind).is_ok() {
                n += 1;
            }
        }
        FixToggle::Fixed(n)
    }

    /// Legt eine Abhängigkeit an. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn add_constraint(&mut self, kind: ConstraintKind) -> Result<ConstraintId, SketchError> {
        if matches!(
            kind,
            ConstraintKind::ArcRadius(_)
                | ConstraintKind::EllipticArcEnds(_)
                | ConstraintKind::InPlane(_)
                | ConstraintKind::PatternValue(..)
                | ConstraintKind::SplineCurvatureHandle(..)
        ) {
            return Err(SketchError::WrongType(tr!(
                "sketch-err-internal-constraint"
            )));
        }
        self.check_refs(&kind)?;
        self.check_centerline(&kind)?;
        self.refuse_text_glyphs(&kind)?;
        let lift = self.check_3d(&kind)?;
        // Erst nach allen Prüfungen anlegen, damit ein Fehler nichts hinterlässt.
        let (mut data, start) = self.new_constraint_data(kind);
        // Mit Mustern: Schlösse die Abhängigkeit eine Schleife zwischen Vorlage und eigenem
        // Exemplar, wird sie abgelehnt (Entscheidung 6).
        if self.data_closes_pattern_cycle(&data, start) {
            return Err(PatternProblem::Cycle.error());
        }
        // Liest sie nur Exemplare, hätte sie keine Wirkung (Schlusskorrektur I1): Ein Maß wird
        // anzeigend und zeigt den gemessenen Wert (wie Fusion bei einem überzähligen Maß), eine
        // geometrische Abhängigkeit wird abgelehnt.
        if self.data_reads_only_instances(&data, start) {
            if !data.kind.is_dimension() {
                return Err(PatternProblem::InstanceOnly.error());
            }
            data.driven = true;
            if let Some(kind) = self
                .measure(&data.kind)
                .and_then(|v| data.kind.with_value(v))
                .filter(|k| self.check_constraint_value(k).is_ok())
            {
                data.kind = kind;
            }
        }
        // 2D-Punkte, die die 3D-Abhängigkeit verbindet, werden 3D (Spec 10). Das legt Variablen
        // an, darum erst danach die Zusatzvariable.
        self.apply_lift(&lift);
        data.extra = start.map(|t| self.new_var(t));
        self.touch_structure();
        Ok(self.constraints.push(data))
    }

    /// Die Daten einer neuen Abhängigkeit `kind` (geprüft) mit der Lage aus der aktuellen
    /// Geometrie, dazu der Startwert ihrer Zusatzvariable. `extra` zeigt auf die nächste freie
    /// Variable; angelegt ist sie noch nicht.
    fn new_constraint_data(&self, kind: ConstraintKind) -> (ConstraintData, Option<f64>) {
        let branch = self.initial_branch(&kind);
        let shared_sign = self.shared_end_sign(&kind);
        let start = self.extra_start(&kind);
        let data = ConstraintData {
            kind,
            branch,
            shared_sign,
            driven: false,
            enabled: true,
            internal: false,
            extra: start.map(|_| self.vars.len()),
            alive: true,
        };
        (data, start)
    }

    /// Schlösse die neue Abhängigkeit `data` eine Schleife zwischen Vorlage und eigenem Exemplar
    /// (Entscheidung 6)? Ohne Muster `false` ohne Rechnung. Die Zusatzvariable ist neu und
    /// verbindet nichts; sie zählt für die Prüfung nicht.
    fn data_closes_pattern_cycle(&self, data: &ConstraintData, start: Option<f64>) -> bool {
        if self.patterns().next().is_none() {
            return false;
        }
        self.pattern_cycle(&self.data_locals(data, start))
    }

    /// Die Variablen der neuen Abhängigkeit `data` ohne ihre (neue) Zusatzvariable.
    fn data_locals(&self, data: &ConstraintData, start: Option<f64>) -> Vec<usize> {
        let mut x = self.vars.clone();
        x.extend(start);
        let mut locals = self.evaluate_data(data, &x).locals;
        locals.retain(|&v| v < self.vars.len());
        locals
    }

    /// Läse die neue Abhängigkeit `data` nur Exemplare eines Musters (Schlusskorrektur I1)? Ohne
    /// Muster `false` ohne Rechnung.
    fn data_reads_only_instances(&self, data: &ConstraintData, start: Option<f64>) -> bool {
        if self.patterns().next().is_none() {
            return false;
        }
        self.reads_only_instances(&self.data_locals(data, start))
    }

    /// Sind die Variablen `locals` alle gehaltene Exemplarvariablen? Dann hätte eine
    /// Abhängigkeit darauf keine Wirkung: Der Löser hält Exemplare fest, sie folgen der Vorlage.
    fn reads_only_instances(&self, locals: &[usize]) -> bool {
        if locals.is_empty() || self.patterns().next().is_none() {
            return false;
        }
        let held = self.pattern_vars();
        locals.iter().all(|v| held.contains(v))
    }

    /// Wird das geprüfte Maß `kind`, angelegt mit [`Sketch::add_constraint`], anzeigend, weil es
    /// nur Exemplare liest? Dieselbe Prüfung, ohne die Skizze zu ändern (für die Vorschau).
    pub(crate) fn plans_driven(&self, kind: &ConstraintKind) -> bool {
        if self.patterns().next().is_none() {
            return false;
        }
        let (data, start) = self.new_constraint_data(kind.clone());
        self.data_reads_only_instances(&data, start)
    }

    /// Liest die bestehende (nicht interne) Abhängigkeit `id` nur Exemplare? Dann wäre sie
    /// treibend ohne Wirkung: `PatternProblem::InstanceOnly` (Schlusskorrektur I1).
    fn check_not_instance_only(&self, id: ConstraintId) -> Result<(), SketchError> {
        if self.constraints[id].internal || self.patterns().next().is_none() {
            return Ok(());
        }
        let Some(ev) = self.evaluate(id, &self.vars) else {
            return Ok(());
        };
        if self.reads_only_instances(&ev.locals) {
            Err(PatternProblem::InstanceOnly.error())
        } else {
            Ok(())
        }
    }

    /// Lehnte [`Sketch::add_constraint`] die geprüfte Abhängigkeit `kind` wegen einer Schleife
    /// zwischen Vorlage und eigenem Exemplar ab? Dieselbe Prüfung, ohne die Skizze zu ändern;
    /// ohne Muster kostet sie nichts.
    pub(crate) fn closes_pattern_cycle(&self, kind: &ConstraintKind) -> bool {
        if self.patterns().next().is_none() {
            return false;
        }
        let (data, start) = self.new_constraint_data(kind.clone());
        self.data_closes_pattern_cycle(&data, start)
    }

    /// Legt einen Musterwert an (nur `add_pattern`). Prüft Wert und Feld.
    pub(crate) fn push_pattern_value(
        &mut self,
        kind: ConstraintKind,
    ) -> Result<ConstraintId, SketchError> {
        self.check_refs(&kind)?;
        self.touch_structure();
        Ok(self.constraints.push(ConstraintData {
            kind,
            branch: 0,
            shared_sign: 0,
            driven: false,
            enabled: true,
            internal: false,
            extra: None,
            alive: true,
        }))
    }

    /// Schlösse die (gerade nicht aktive) Abhängigkeit `id`, wieder aktiv, eine Schleife zwischen
    /// Vorlage und eigenem Exemplar? Dann `PatternProblem::Cycle` (Entscheidung 6). Gilt auch für
    /// die interne Abhängigkeit einer neuen Kurve (`add_arc`, `add_elliptic_arc`).
    pub(crate) fn check_activation_cycle(&self, id: ConstraintId) -> Result<(), SketchError> {
        if self.patterns().next().is_none() {
            return Ok(());
        }
        let Some(ev) = self.evaluate(id, &self.vars) else {
            return Ok(());
        };
        if self.pattern_cycle(&ev.locals) {
            Err(PatternProblem::Cycle.error())
        } else {
            Ok(())
        }
    }

    /// Musterwerte bleiben, solange ihr Muster lebt: nicht löschen, nicht anzeigend schalten.
    fn check_not_pattern_value(&self, id: ConstraintId) -> Result<(), SketchError> {
        if matches!(self.constraints[id].kind, ConstraintKind::PatternValue(..)) {
            Err(SketchError::InvalidInput(tr!("sketch-pattern-value-keep")))
        } else {
            Ok(())
        }
    }

    /// Das `Fixed` am Ende eines lebenden festen Splines bleibt (A3): Ohne es verließe der Punkt die
    /// unveränderliche Kurve. Mit der Kurve geht es.
    fn check_not_fixed_spline_end(&self, id: ConstraintId) -> Result<(), SketchError> {
        let ConstraintKind::Fixed(p, ..) = self.constraints[id].kind else {
            return Ok(());
        };
        let holds = |d: &crate::model::CurveData| matches!(d.kind, CurveKind::FixedSpline { start, end, .. } if p == start || p == end);
        if self.curves().any(|(_, d)| holds(d)) {
            Err(SketchError::InvalidInput(tr!(
                "sketch-fixed-spline-keep-fixed"
            )))
        } else {
            Ok(())
        }
    }

    /// Ein neuer Durchmesser zur Mittellinie braucht eine Mittellinie. Nur beim Anlegen und
    /// Umschalten geprüft: Wer die Mittellinie später ausschaltet, behält ein lösbares Maß, und
    /// eine solche Datei lädt weiter.
    fn check_centerline(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        match kind {
            ConstraintKind::CenterlineDiameter(_, l, _) if !self.curves[*l].centerline => {
                Err(SketchError::WrongType(tr!("sketch-diameter-centerline")))
            }
            _ => Ok(()),
        }
    }

    /// Schaltet ein Maß zwischen Radius und Durchmesser um: `Radius(c, v) ↔ Diameter(c, 2v)`,
    /// `DistancePointLine(p, L, v) ↔ CenterlineDiameter(p, L, 2v)` (nur an Mittellinien). Id,
    /// `branch`, `driven` und `enabled` bleiben; zweimal umgeschaltet ist der Wert bitgleich. Bei
    /// einem Fehler bleibt die Skizze unverändert.
    pub fn toggle_radius_diameter(&mut self, id: ConstraintId) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        let kind = toggled_kind(&self.constraints[id].kind)
            .ok_or_else(|| SketchError::WrongType(tr!("sketch-err-toggle-kinds")))?;
        self.check_refs(&kind)?;
        self.check_centerline(&kind)?;
        self.constraints[id].kind = kind;
        self.touch_structure();
        Ok(())
    }

    /// Kann die Kurve `c` an einem Glatt teilhaben (Spec C5)? Für die Anzeige, solange die zweite
    /// Kurve fehlt; auch gelöschte Kurven zählen nach ihrer Art.
    pub fn smooth_partner(&self, c: CurveId) -> bool {
        self.curves
            .get(c)
            .is_some_and(|d| smooth_partner_kind(&d.kind))
    }

    /// Geht Glatt zwischen `a` und `b` (Spec C5)? Mindestens eine Seite ist ein Spline einer Art, die
    /// andere ein gültiger Partner; zwei feste Splines gehen nicht, ein fester Spline und eine Linie
    /// auch nicht ([`smooth_fixed_against_line`](Self::smooth_fixed_against_line)). Dieselbe Regel
    /// nutzen die Prüfung und die Oberfläche.
    pub fn smooth_kinds_fit(&self, a: CurveId, b: CurveId) -> bool {
        let (Some(ka), Some(kb)) = (self.curves.get(a), self.curves.get(b)) else {
            return false;
        };
        let (ka, kb) = (&ka.kind, &kb.kind);
        a != b
            && smooth_partner_kind(ka)
            && smooth_partner_kind(kb)
            && (smooth_leader(ka) || smooth_leader(kb))
            && !(matches!(ka, CurveKind::FixedSpline { .. })
                && matches!(kb, CurveKind::FixedSpline { .. }))
            && !self.smooth_fixed_against_line(a, b)
    }

    /// Sind `a` und `b` ein fester Spline und eine Linie, in beliebiger Folge? Die Krümmung des festen
    /// Splines ändert sich nie, die der Linie ist 0: Die Krümmungszeile ginge nicht auf, und keine
    /// Variable könnte helfen. Die Regel gilt nach der Art, auch für ein Ende, das zufällig gerade
    /// ausläuft; sonst hinge der Befehl an einem Wert, den man nicht sieht.
    pub fn smooth_fixed_against_line(&self, a: CurveId, b: CurveId) -> bool {
        let (Some(ka), Some(kb)) = (self.curves.get(a), self.curves.get(b)) else {
            return false;
        };
        matches!(
            (&ka.kind, &kb.kind),
            (CurveKind::FixedSpline { .. }, CurveKind::Line { .. })
                | (CurveKind::Line { .. }, CurveKind::FixedSpline { .. })
        )
    }

    /// Das wirksame Glatt zwischen genau diesen Enden, in beiden Reihenfolgen.
    pub fn smooth_between(
        &self,
        a: CurveId,
        pa: PointId,
        b: CurveId,
        pb: PointId,
    ) -> Option<ConstraintId> {
        self.constraints().find_map(|(id, d)| match d.kind {
            ConstraintKind::Smooth(x, px, y, py)
                if d.enabled
                    && ((x, px, y, py) == (a, pa, b, pb) || (x, px, y, py) == (b, pb, a, pa)) =>
            {
                Some(id)
            }
            _ => None,
        })
    }

    /// Die andere Seite eines wirksamen Glatt am Ende `at` der Kurve `c`.
    pub(crate) fn smooth_partner_at(&self, c: CurveId, at: PointId) -> Option<(CurveId, PointId)> {
        self.constraints().find_map(|(_, d)| match d.kind {
            ConstraintKind::Smooth(x, px, y, py) if d.enabled => {
                if (x, px) == (c, at) {
                    Some((y, py))
                } else if (y, py) == (c, at) {
                    Some((x, px))
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    /// Richtung (nicht normiert) aus dem Ende `p` in die Kurve `c` und die Krümmung dort, in Zahlen.
    pub(crate) fn smooth_end_values(&self, c: CurveId, p: PointId) -> Option<(DVec2, f64)> {
        let ctx = Ctx::collecting(self, &self.vars);
        let u = into_curve(&ctx, c, p)?;
        let k = end_curvature_dual(&ctx, c, p)?;
        Some((DVec2::new(u.0.v, u.1.v), k.v))
    }

    /// Sind die Punkte eines Glatt noch Enden ihrer Kurven? `true` für alle anderen Arten. Nach
    /// Stutzen, Brechen oder Rundung kann ein Punkt aufhören, Ende zu sein; dann entfällt Glatt
    /// (`settle_constraints`), sonst bliebe ein lebendes Glatt ohne Zeile, das `add_constraint`
    /// ablehnen würde.
    pub(crate) fn smooth_ends_hold(&self, kind: &ConstraintKind) -> bool {
        let ConstraintKind::Smooth(a, pa, b, pb) = *kind else {
            return true;
        };
        let is_end = |c: CurveId, p: PointId| {
            curve_ends(&self.curves[c].kind).is_some_and(|e| e.contains(&p))
        };
        is_end(a, pa) && is_end(b, pb)
    }

    /// Die Enden, an denen Glatt die Kurven `a` und `b` verbindet. Hängen sie schon an Enden
    /// zusammen (gemeinsamer Punkt oder `Coincident`), gilt diese Fuge; bei mehreren die nächst den
    /// Klicks. Sonst wie in Fusion an der Klickstelle: Wurde eine Kurve per Klick gewählt (`at_a`, `at_b`), gilt ihr Ende, das dem
    /// Klick am nächsten liegt; ohne Klick (etwa Rahmenauswahl) das Ende, das am nächsten an einem
    /// Ende der anderen Kurve liegt (ein gemeinsames Ende zuerst). Die Weite ist nicht begrenzt:
    /// Getrennte Enden legt der Befehl der Oberfläche mit einem `Coincident` zusammen. Die Wahl
    /// hängt nicht von der Reihenfolge ab (Gleichstand: zuerst ein wirklich gemeinsamer Punkt vor
    /// zwei deckenden, dann das Paar mit den kleineren Punktnummern). `None` für dieselbe Kurve
    /// oder Kurven ohne Enden (wie bei [`curve_ends`]). Die Kurvenarten prüft
    /// [`Sketch::add_constraint`]: Auch eine Linie und ein Bogen liefern Enden.
    pub fn smooth_ends(
        &self,
        a: CurveId,
        at_a: Option<DVec2>,
        b: CurveId,
        at_b: Option<DVec2>,
    ) -> Option<(PointId, PointId)> {
        if a == b {
            return None;
        }
        let (ea, eb) = (
            curve_ends(&self.curve(a)?.kind)?,
            curve_ends(&self.curve(b)?.kind)?,
        );
        let dist = |p: PointId, q: DVec2| self.pos(p).distance(q);
        // Mit Klick bleibt nur das Ende nächst dem Klick.
        let clicked = |ends: [PointId; 2], at: Option<DVec2>| -> Vec<PointId> {
            match at {
                Some(q) => ends
                    .into_iter()
                    .min_by(|x, y| dist(*x, q).total_cmp(&dist(*y, q)).then(x.cmp(y)))
                    .into_iter()
                    .collect(),
                None => ends.to_vec(),
            }
        };
        let gap = |p: PointId, q: PointId| {
            if p == q { 0.0 } else { dist(p, self.pos(q)) }
        };
        let key = |p: PointId, q: PointId| (p.min(q), p.max(q));
        // Hängen die Kurven schon an Enden zusammen, gilt eine solche Fuge vor dem Klick. Das Ende
        // nächst dem Klick legte sonst das freie Ende auf die Fuge, und eine Kurve schlösse sich zum
        // Vollkreis (Handtest Skizzen 7). Rang: gemeinsamer Punkt, Kette aus „Deckungsgleich“, gleiche
        // Lage ohne Abhängigkeit (bis `tol.linear`). Der Klick wählt nur noch zwischen Fugen
        // desselben Rangs.
        let rank = |p: PointId, q: PointId| -> Option<u8> {
            if p == q {
                Some(0)
            } else if self.coincident_chain(p, q) {
                Some(1)
            } else if gap(p, q) <= self.tol.linear {
                Some(2)
            } else {
                None
            }
        };
        let joints: Vec<(u8, PointId, PointId)> = ea
            .iter()
            .flat_map(|&p| eb.iter().map(move |&q| (p, q)))
            .filter_map(|(p, q)| rank(p, q).map(|r| (r, p, q)))
            .collect();
        if !joints.is_empty() {
            let off = |p: PointId, at: Option<DVec2>| at.map_or(0.0, |q| dist(p, q));
            return joints
                .into_iter()
                .min_by(|&(ra, p, q), &(rb, r, t)| {
                    ra.cmp(&rb)
                        .then_with(|| {
                            (off(p, at_a) + off(q, at_b)).total_cmp(&(off(r, at_a) + off(t, at_b)))
                        })
                        .then_with(|| key(p, q).cmp(&key(r, t)))
                })
                .map(|(_, p, q)| (p, q));
        }
        let (ca, cb) = (clicked(ea, at_a), clicked(eb, at_b));
        ca.iter()
            .flat_map(|&p| cb.iter().map(move |&q| (p, q)))
            .min_by(|&(p, q), &(r, t)| {
                gap(p, q)
                    .total_cmp(&gap(r, t))
                    // Ein wirklich gemeinsamer Punkt vor zwei deckenden Punkten.
                    .then_with(|| (p != q).cmp(&(r != t)))
                    .then_with(|| key(p, q).cmp(&key(r, t)))
            })
    }

    /// Sind `p` und `q` über eine Kette eingeschalteter `Coincident` verbunden (auch direkt)? Für
    /// Glatt: Eine solche Fuge braucht keine weitere Lagebedingung, sie wäre überzählig.
    pub fn coincident_chain(&self, p: PointId, q: PointId) -> bool {
        if p == q {
            return true;
        }
        let pairs: Vec<(PointId, PointId)> = self
            .constraints()
            .filter(|(_, k)| k.enabled)
            .filter_map(|(_, k)| match k.kind {
                ConstraintKind::Coincident(x, y) => Some((x, y)),
                _ => None,
            })
            .collect();
        let mut seen = vec![p];
        let mut open = vec![p];
        while let Some(x) = open.pop() {
            for &(a, b) in &pairs {
                let next = if a == x {
                    b
                } else if b == x {
                    a
                } else {
                    continue;
                };
                if next == q {
                    return true;
                }
                if !seen.contains(&next) {
                    seen.push(next);
                    open.push(next);
                }
            }
        }
        false
    }

    /// Entfernt eine (nicht interne) Abhängigkeit.
    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.check_not_pattern_value(id)?;
        self.check_not_fixed_spline_end(id)?;
        self.constraints[id].alive = false;
        self.touch_structure();
        Ok(())
    }

    /// Entfernt die Kurve samt allen Abhängigkeiten, die sie benutzen (auch interne).
    /// Ist sie Vorlage, Exemplar oder Richtungslinie eines Musters, wird das Muster aufgelöst;
    /// ein einzelnes Exemplar löscht die Oberfläche über [`Sketch::suppress_instance`].
    ///
    /// Griffe (Skizzen 7, B8): Der Spline nimmt seine Griffe mit. Die Griff-Linie schaltet den Griff
    /// aus, der Griff-Bogen nur den Krümmungsgriff; der Spline bleibt und springt auf die Form ohne
    /// sie.
    ///
    /// Text (Skizzen 8, B8): Rahmenlinien und erzeugte Kurven gehen nur mit dem ganzen Text
    /// ([`Sketch::remove_text`]), ein Pfad mit Text gar nicht; dann bleibt alles, wie es ist.
    pub fn remove_curve(&mut self, c: CurveId) -> Result<(), SketchError> {
        self.check_curve(c)?;
        self.refuse_text_curve_removal(c)?;
        let released = self.release_handles_of(c);
        self.dissolve_patterns_of_curve(c);
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
        self.finish_release(released)
    }

    /// Entfernt den Punkt samt allen Kurven und Abhängigkeiten, die ihn benutzen.
    /// Ist er Mitte, Vorlagen- oder Exemplarpunkt eines Musters, wird das Muster aufgelöst.
    /// Eine Griffspitze nimmt nur den Griff mit, nicht den Spline (Skizzen 7). Ecken und erzeugte
    /// Punkte eines Textes gehen nur mit dem ganzen Text, Punkte eines Pfads mit Text gar nicht
    /// (Skizzen 8, B8).
    pub fn remove_point(&mut self, p: PointId) -> Result<(), SketchError> {
        self.check_point(p)?;
        self.refuse_text_point_removal(p)?;
        self.dissolve_patterns_of_point(p);
        self.touch_structure();
        self.points[p].alive = false;
        // Zuerst: Danach gehört `p` nicht mehr zu den Punkten des Splines.
        for part in self.handle_parts_at(p) {
            self.remove_curve(part)?;
        }
        let curves: Vec<CurveId> = self
            .curves()
            .filter(|(_, d)| d.kind.points().contains(&p))
            .map(|(i, _)| i)
            .collect();
        for c in curves {
            // Der Spline an einem Stützpunkt hat seine Griff-Linien schon mitgenommen.
            if self.check_curve(c).is_ok() {
                self.remove_curve(c)?;
            }
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
        // Die Mitte eines Kreismusters ist benutzt, auch als freier Punkt.
        used.extend(self.pattern_centers());
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

#[cfg(test)]
mod tests {
    use super::*;
    use planora_math::{Frame, Tolerance};

    /// Wert und Gradient am Stand `x`; der Gradient nach `locals` wie in `evaluate_data`.
    fn end_kappa(s: &Sketch, c: CurveId, p: PointId, x: &[f64]) -> (f64, Vec<(usize, f64)>) {
        let collect = Ctx::collecting(s, x);
        end_curvature_dual(&collect, c, p).unwrap();
        let locals = collect.locals.into_inner();
        let ctx = Ctx {
            sk: s,
            x,
            locals: RefCell::new(locals.clone()),
            collecting: false,
        };
        let k = end_curvature_dual(&ctx, c, p).unwrap();
        let grads = locals.iter().enumerate().map(|(i, &v)| (v, k.grad(i)));
        (k.v, grads.collect())
    }

    /// Prüft den Wert gegen `want` und den Gradienten gegen Differenzen.
    fn check(s: &Sketch, c: CurveId, p: PointId, want: f64) {
        let x = s.variables().to_vec();
        let (k, grads) = end_kappa(s, c, p, &x);
        assert!((k - want).abs() < 1e-9, "κ {k}, soll {want}");
        let h = 1e-6;
        for (v, g) in grads {
            let (mut up, mut down) = (x.clone(), x.clone());
            up[v] += h;
            down[v] -= h;
            let fd = (end_kappa(s, c, p, &up).0 - end_kappa(s, c, p, &down).0) / (2.0 * h);
            assert!((fd - g).abs() < 1e-6, "Variable {v}: {g} / Differenz {fd}");
        }
    }

    /// Krümmung aus der Form, in der Richtung in die Kurve hinein.
    fn from_form(s: &Sketch, c: CurveId, first: bool) -> f64 {
        let g = s.geom(c).unwrap();
        let (lo, hi) = g.domain();
        if first {
            g.curvature(lo)
        } else {
            -g.curvature(hi)
        }
    }

    fn sketch() -> Sketch {
        Sketch::new(Frame::WORLD, "S", Tolerance::default())
    }

    fn points(s: &mut Sketch, xy: &[(f64, f64)]) -> Vec<PointId> {
        xy.iter()
            .map(|&(x, y)| s.add_point(x, y).unwrap())
            .collect()
    }

    const WAVE: [(f64, f64); 5] = [
        (0.0, 0.0),
        (10.0, 6.0),
        (21.0, -3.0),
        (30.0, 8.0),
        (42.0, 5.0),
    ];

    #[test]
    fn line_and_arc_at_their_ends() {
        let mut s = sketch();
        let p = points(&mut s, &[(0.0, 0.0), (5.0, 1.0)]);
        let line = s.add_line(p[0], p[1]).unwrap();
        check(&s, line, p[0], 0.0);
        check(&s, line, p[1], 0.0);
        // Bogen gegen den Uhrzeigersinn: vom Start aus links, vom Ende aus rechts.
        let q = points(&mut s, &[(1.0, 1.0), (5.0, 1.0), (1.0, 5.0)]);
        let arc = s.add_arc(q[0], q[1], q[2]).unwrap();
        check(&s, arc, q[1], 0.25);
        check(&s, arc, q[2], -0.25);
        // Ein Punkt, der kein Ende ist.
        let x = Ctx::collecting(&s, s.variables());
        assert!(end_curvature_dual(&x, arc, q[0]).is_none());
        assert!(end_curvature_dual(&x, line, q[1]).is_none());
    }

    #[test]
    fn spline_without_a_handle_is_natural_at_its_end() {
        let mut s = sketch();
        let p = points(&mut s, &WAVE);
        let c = s.add_spline(&p).unwrap();
        check(&s, c, p[0], 0.0);
        check(&s, c, p[4], 0.0);
        let x = Ctx::collecting(&s, s.variables());
        assert!(end_curvature_dual(&x, c, p[2]).is_none(), "innerer Punkt");
    }

    /// Spline mit Griffen an beiden Enden; die Spitzen gedreht, sonst ist die Krümmung dort null.
    fn turned_spline(s: &mut Sketch) -> (CurveId, Vec<PointId>) {
        let p = points(s, &WAVE);
        let c = s.add_spline(&p).unwrap();
        s.activate_tangent_handle(c, p[0]).unwrap();
        s.activate_tangent_handle(c, p[4]).unwrap();
        let CurveKind::Spline { handles, .. } = s.curves[c].kind.clone() else {
            panic!("kein Spline");
        };
        s.set_point_xy(handles[0].tip, 1.0, 7.0).unwrap();
        s.set_point_xy(handles[1].tip, 44.0, 12.0).unwrap();
        (c, p)
    }

    #[test]
    fn spline_with_form_handle_uses_the_form() {
        let mut s = sketch();
        let (c, p) = turned_spline(&mut s);
        let (a, b) = (from_form(&s, c, true), from_form(&s, c, false));
        assert!(a.abs() > 1e-4 && b.abs() > 1e-4);
        check(&s, c, p[0], a);
        check(&s, c, p[4], b);
    }

    #[test]
    fn spline_with_arc_handle_gives_the_handle_value() {
        let mut s = sketch();
        let (c, p) = turned_spline(&mut s);
        for at in [p[0], p[4]] {
            let form = from_form(&s, c, at == p[0]);
            s.activate_curvature_handle(c, at).unwrap();
            assert!((form - from_form(&s, c, at == p[0])).abs() < 1e-9);
            check(&s, c, at, form);
        }
    }

    #[test]
    fn control_spline_and_conic_come_from_their_form() {
        for (degree, xy) in [(5_u8, &WAVE[..]), (3, &WAVE[..]), (3, &WAVE[..3])] {
            let mut s = sketch();
            let p = points(&mut s, xy);
            let c = s.add_control_spline(&p, degree).unwrap();
            check(&s, c, p[0], from_form(&s, c, true));
            check(&s, c, *p.last().unwrap(), from_form(&s, c, false));
        }
        // Ungleichmäßige Knoten: ein eingefügter Punkt nahe dem Anfang und dem Ende.
        let mut s = sketch();
        let p = points(&mut s, &WAVE);
        let c = s.add_control_spline(&p, 3).unwrap();
        for t in [0.05, 0.95] {
            let (lo, hi) = s.curve_domain(c).unwrap();
            let q = s.curve_eval(c, lo + t * (hi - lo)).unwrap();
            s.insert_control_point(c, q).unwrap();
        }
        check(&s, c, p[0], from_form(&s, c, true));
        check(&s, c, p[4], from_form(&s, c, false));
        // Zwei Kontrollpunkte: gerade, Krümmung null.
        let mut s = sketch();
        let p = points(&mut s, &[(0.0, 0.0), (5.0, 5.0)]);
        let c = s.add_control_spline(&p, 5).unwrap();
        check(&s, c, p[0], 0.0);
        check(&s, c, p[1], 0.0);
        // Kegelschnitte: Ellipse, Parabel, Hyperbel.
        for rho in [0.3, 0.5, 0.8] {
            let mut s = sketch();
            let p = points(&mut s, &[(0.0, 0.0), (10.0, 0.0), (3.0, 7.0)]);
            let c = s.add_conic(p[0], p[1], p[2], rho).unwrap();
            let (a, b) = (from_form(&s, c, true), from_form(&s, c, false));
            // Beide Enden drehen zum Scheitel hin, also in Laufrichtung gegensinnig.
            assert!(a.abs() > 1e-3 && a.signum() != b.signum(), "{a} / {b}");
            check(&s, c, p[0], a);
            check(&s, c, p[1], b);
        }
    }
}
