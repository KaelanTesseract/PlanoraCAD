//! 3D-Fassung der Abhängigkeiten (Skizzen 10). Sobald ein 3D-Punkt oder eine Kurve mit eigener
//! Normale beteiligt ist ([`Sketch::constraint_is_3d`]), rechnen die Arten aus der Fusion-Liste
//! hier im Raum. In 2D-Lage (alles bei z = 0, Normale z) haben die Residuen dieselben Nullstellen
//! wie die 2D-Fassung in `constraint.rs`; dazu kommen Zeilen, die dort von selbst null sind (etwa
//! der z-Anteil). Alle übrigen Arten sind an 3D-Geometrie gesperrt
//! ([`ConstraintKind::allowed_in_3d`]); liegen sie trotzdem vor (interne Einträge, Laden), rechnen
//! sie wie bisher in der Draufsicht.

use crate::constraint::{
    ConstraintData, ConstraintKind, Ctx, LineOrPoints, MIN_LEN, collinear_point, extra_var,
    is_line_kind, line_and_param, lines_share_point, shared_end,
};
use crate::curves::{
    CONIC_KNOTS, conic_weight, nurbs_eval_dual, spline_eval, spline_second_derivatives,
};
use crate::dual::Dual;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use crate::spline_handles::{hermite_eval_dual, nodes_of};
use planora_i18n::tr;
use planora_math::{DVec3, TAU};

pub(crate) type V3 = (Dual, Dual, Dual);

/// Achse der Skizze, zu der eine 3D-Linie waagerecht/senkrecht gestellt wird (Annahme A3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis3 {
    X,
    Y,
    Z,
}

/// Abtastschritte je Parameterabschnitt beim Startwert der Zusatzvariablen im Raum.
const SAMPLES_PER_PIECE: usize = 64;
/// Schritte der Dreiteilung beim Nachschärfen des Startwerts.
const REFINE_STEPS: usize = 80;

// ---- Vektorrechnung -----------------------------------------------------------------------

pub(crate) fn sub3(a: &V3, b: &V3) -> V3 {
    (&a.0 - &b.0, &a.1 - &b.1, &a.2 - &b.2)
}
fn add3(a: &V3, b: &V3) -> V3 {
    (&a.0 + &b.0, &a.1 + &b.1, &a.2 + &b.2)
}
fn scale3(a: &V3, k: &Dual) -> V3 {
    (&a.0 * k, &a.1 * k, &a.2 * k)
}
pub(crate) fn dot3(a: &V3, b: &V3) -> Dual {
    &a.0 * &b.0 + &a.1 * &b.1 + &a.2 * &b.2
}
/// Skalarprodukt mit einer festen Richtung (etwa der Normale einer Kurve).
pub(crate) fn dot3c(a: &V3, n: DVec3) -> Dual {
    &a.0 * n.x + &a.1 * n.y + &a.2 * n.z
}
fn cross3(a: &V3, b: &V3) -> V3 {
    (
        &a.1 * &b.2 - &a.2 * &b.1,
        &a.2 * &b.0 - &a.0 * &b.2,
        &a.0 * &b.1 - &a.1 * &b.0,
    )
}
/// Kreuzprodukt `n × a` einer festen Richtung mit einem Vektor.
pub(crate) fn cross3c(n: DVec3, a: &V3) -> V3 {
    (
        &a.2 * n.y - &a.1 * n.z,
        &a.0 * n.z - &a.2 * n.x,
        &a.1 * n.x - &a.0 * n.y,
    )
}
pub(crate) fn length3(a: &V3) -> Dual {
    dot3(a, a).sqrt()
}
/// Länge, nach unten auf `MIN_LEN` begrenzt (zum Normieren).
fn norm_len3(a: &V3) -> Dual {
    let l = length3(a);
    if l.v < MIN_LEN {
        Dual::constant(MIN_LEN)
    } else {
        l
    }
}
fn comp(a: &V3, k: usize) -> &Dual {
    match k {
        0 => &a.0,
        1 => &a.1,
        _ => &a.2,
    }
}

/// Die zwei Anteile von `d × w`, die nicht zur größten Komponente von `d` gehören. `d × w` steht
/// senkrecht auf `d`, hat also nur zwei unabhängige Anteile; der weggelassene ist der, der bei
/// `d` längs dieser Achse ohnehin (fast) null ist. Ist `d` waagerecht (z-Anteil 0), bleibt der
/// z-Anteil immer dabei: das 2D-Kreuzprodukt.
fn perp2(d: &V3, w: &V3) -> [Dual; 2] {
    let c = cross3(d, w);
    let a = [d.0.v.abs(), d.1.v.abs(), d.2.v.abs()];
    let k = if a[0] >= a[1] && a[0] >= a[2] {
        0
    } else if a[1] >= a[2] {
        1
    } else {
        2
    };
    let (i, j) = match k {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    [comp(&c, i).clone(), comp(&c, j).clone()]
}

// ---- Kurven im Raum -----------------------------------------------------------------------

/// Anfangspunkt und Richtung `b − a` einer (geprüften) Linie im Raum.
fn line_dir3(ctx: &Ctx, c: CurveId) -> (V3, V3) {
    let CurveKind::Line { a, b } = ctx.sk.curves[c].kind else {
        unreachable!("type was checked")
    };
    let pa = ctx.pt3(a);
    let d = sub3(&ctx.pt3(b), &pa);
    (pa, d)
}

/// Mittelpunkt, Radius und Normale eines (geprüften) Kreises oder Bogens im Raum.
fn circle_of3(ctx: &Ctx, c: CurveId) -> (V3, Dual, DVec3) {
    let n = ctx.sk.curve_normal(c);
    match &ctx.sk.curves[c].kind {
        CurveKind::Circle { center, radius } => (ctx.pt3(*center), ctx.var(*radius), n),
        CurveKind::Arc { center, start, .. } => {
            let m = ctx.pt3(*center);
            let r = length3(&sub3(&ctx.pt3(*start), &m));
            (m, r, n)
        }
        _ => unreachable!("type was checked"),
    }
}

/// Große Halbachse einer Ellipse oder eines Ellipsenbogens im Raum; ohne `az` mit z als
/// Konstante 0.
pub(crate) fn ellipse_axis3(ctx: &Ctx, ax: usize, ay: usize, az: Option<usize>) -> V3 {
    let z = az.map_or_else(|| Dual::constant(0.0), |i| ctx.var(i));
    (ctx.var(ax), ctx.var(ay), z)
}

/// Mitte, große Halbachse, deren (nach unten begrenzte) Länge und kleine Halbachse einer
/// (geprüften) Ellipse oder eines Ellipsenbogens im Raum.
fn ellipse_of3(ctx: &Ctx, c: CurveId) -> (V3, V3, Dual, Dual) {
    let (CurveKind::Ellipse {
        center,
        ax,
        ay,
        az,
        minor,
    }
    | CurveKind::EllipticArc {
        center,
        ax,
        ay,
        az,
        minor,
        ..
    }) = ctx.sk.curves[c].kind
    else {
        unreachable!("type was checked")
    };
    let a = ellipse_axis3(ctx, ax, ay, az);
    let len = norm_len3(&a);
    (ctx.pt3(center), a, len, ctx.var(minor))
}

/// Abstand des Punktes `p` von der Ellipse in impliziter Form wie `ellipse_residual`:
/// Achskoordinaten über die große Halbachse und `n × a` in der Ebene der Kurve. In der
/// Skizzenebene ist `n × a = (−ay, ax, 0)`, also dieselbe Rechnung.
pub(crate) fn ellipse_residual3(ctx: &Ctx, c: CurveId, p: &V3) -> Dual {
    let n = ctx.sk.curve_normal(c);
    let (m, a, len, minor) = ellipse_of3(ctx, c);
    let minor = if minor.v < MIN_LEN {
        Dual::constant(MIN_LEN)
    } else {
        minor
    };
    let v = sub3(p, &m);
    let x = dot3(&v, &a) / &len / &len;
    let y = dot3(&v, &cross3c(n, &a)) / &len / &minor;
    let r = (&x * &x + &y * &y).sqrt();
    (r - 1.0) * (&len + &minor) * 0.5
}

/// z als natürlicher Spline durch die Stützpunkte (wie in [`Sketch::curve_point3`]).
fn spline_z(ctx: &Ctx, points: &[PointId], t: &Dual) -> (Dual, Dual) {
    let zs: Vec<Dual> = points.iter().map(|&p| ctx.pt3(p).2).collect();
    spline_eval(&zs, &spline_second_derivatives(&zs), t)
}

/// NURBS im Raum: (x, y) und z getrennt, denn bei festen Knoten und Gewichten rechnet jede
/// Koordinate für sich.
fn nurbs3(degree: usize, knots: &[f64], weights: Option<&[f64]>, pts: &[V3], t: &Dual) -> (V3, V3) {
    let xy: Vec<(Dual, Dual)> = pts.iter().map(|p| (p.0.clone(), p.1.clone())).collect();
    let z: Vec<(Dual, Dual)> = pts
        .iter()
        .map(|p| (p.2.clone(), Dual::constant(0.0)))
        .collect();
    let (q, dq) = nurbs_eval_dual(degree, knots, weights, &xy, t);
    let (qz, dz) = nurbs_eval_dual(degree, knots, weights, &z, t);
    ((q.0, q.1, qz.0), (dq.0, dq.1, dz.0))
}

/// Punkt und Ableitung nach `t` einer (geprüften) Ellipse, eines Ellipsenbogens (auf seiner
/// ganzen Ellipse), eines Splines oder einer Freiformkurve im Raum; dieselben Parameter wie
/// `param_curve` und [`Sketch::curve_point3`].
fn param_curve3(ctx: &Ctx, c: CurveId, t: &Dual) -> (V3, V3) {
    match &ctx.sk.curves[c].kind {
        CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
            let n = ctx.sk.curve_normal(c);
            let (m, a, len, minor) = ellipse_of3(ctx, c);
            let b = scale3(&cross3c(n, &a), &(minor / &len));
            let (ct, st) = (t.cos(), t.sin());
            let p = add3(&add3(&m, &scale3(&a, &ct)), &scale3(&b, &st));
            let d = sub3(&scale3(&b, &ct), &scale3(&a, &st));
            (p, d)
        }
        CurveKind::Spline { points, handles } if !handles.is_empty() => {
            // Griffe gibt es nur in der Draufsicht; z folgt dem natürlichen Spline.
            let (q, dq) = hermite_eval_dual(&nodes_of(points, handles, |p| ctx.pt(p)), t);
            let (z, dz) = spline_z(ctx, points, t);
            ((q.0, q.1, z), (dq.0, dq.1, dz))
        }
        CurveKind::Spline { points, .. } => {
            let pts: Vec<V3> = points.iter().map(|&p| ctx.pt3(p)).collect();
            let coord = |k: usize| -> (Dual, Dual) {
                let ys: Vec<Dual> = pts.iter().map(|p| comp(p, k).clone()).collect();
                spline_eval(&ys, &spline_second_derivatives(&ys), t)
            };
            let ((x, dx), (y, dy), (z, dz)) = (coord(0), coord(1), coord(2));
            ((x, y, z), (dx, dy, dz))
        }
        CurveKind::ControlSpline {
            points,
            degree,
            knots,
        } => {
            let pts: Vec<V3> = points.iter().map(|&p| ctx.pt3(p)).collect();
            nurbs3(usize::from(*degree), knots, None, &pts, t)
        }
        CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        } => {
            let pts = [ctx.pt3(*start), ctx.pt3(*apex), ctx.pt3(*end)];
            let weights = [1.0, conic_weight(*rho), 1.0];
            nurbs3(2, &CONIC_KNOTS, Some(&weights), &pts, t)
        }
        CurveKind::FixedSpline { curve, .. } => {
            let k = |v: f64| Dual::constant(v);
            let pts: Vec<V3> = curve
                .control_points()
                .iter()
                .map(|q| (k(q.x), k(q.y), k(q.z)))
                .collect();
            nurbs3(
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

/// Zwei Zeilen „Punkt `p` auf der Geraden durch `a` mit Richtung `d`“ (Abstand je Achse).
fn on_line_rows(a: &V3, d: &V3, p: &V3) -> [Dual; 2] {
    let len = norm_len3(d);
    let [r0, r1] = perp2(d, &sub3(p, a));
    [r0 / &len, r1 / &len]
}

/// Zwei Zeilen „`d2` parallel zu `d1`“ (Sinus des Winkels je Achse).
fn parallel_rows(d1: &V3, d2: &V3) -> [Dual; 2] {
    let den = norm_len3(d1) * norm_len3(d2);
    let [r0, r1] = perp2(d1, d2);
    [r0 / &den, r1 / &den]
}

/// Sinus- und Kosinusanteile der Tangente `c1`–`c2` am gemeinsamen Endpunkt `p` im Raum, wie
/// `shared_end_parts`: Der erste Sinusanteil ist null, wenn die Richtung senkrecht zum Radius
/// steht, der zweite, wenn sie in der Bogenebene liegt; der Kosinusanteil sagt, in welcher Lage.
/// In 2D-Lage sind erster Sinus- und Kosinusanteil genau die Werte der 2D-Fassung.
fn shared_end_parts3(
    ctx: &Ctx,
    c1: CurveId,
    c2: CurveId,
    p: PointId,
) -> (Dual, Option<Dual>, Dual) {
    let sk = ctx.sk;
    let at = ctx.pt3(p);
    let line_arc = |line: CurveId, arc: CurveId| {
        let (m, _, n) = circle_of3(ctx, arc);
        let (_, d) = line_dir3(ctx, line);
        let r = sub3(&at, &m);
        let rl = norm_len3(&r);
        // `r × n` ist die Tangente in der Bogenebene; in 2D `d·(r × z) = cross(d, r)`.
        let rxn = (
            &r.1 * n.z - &r.2 * n.y,
            &r.2 * n.x - &r.0 * n.z,
            &r.0 * n.y - &r.1 * n.x,
        );
        let cos = dot3(&d, &rxn);
        (dot3(&d, &r), Some(dot3c(&d, n) * &rl), cos)
    };
    match (&sk.curves[c1].kind, &sk.curves[c2].kind) {
        (CurveKind::Line { .. }, _) => line_arc(c1, c2),
        (_, CurveKind::Line { .. }) => line_arc(c2, c1),
        _ => {
            let ((m1, _, n1), (m2, _, n2)) = (circle_of3(ctx, c1), circle_of3(ctx, c2));
            let (u, v) = (sub3(&m1, &at), sub3(&m2, &at));
            let (t1, t2) = (cross3c(n1, &u), cross3c(n2, &v));
            // `t2` parallel zu `t1` heißt: `t2 ⟂ u` und `t2 ⟂ n1`. In 2D ist
            // `−t2·u = cross(u, v)` und `t1·t2 = u·v`. Bei parallelen Normalen liegt `t2` von
            // selbst senkrecht zu `n1`; die zweite Zeile wäre immer null und fällt weg.
            let off = (!parallel_normals(sk, n1, n2)).then(|| dot3c(&t2, n1) * norm_len3(&u));
            (-dot3(&t2, &u), off, dot3(&t1, &t2))
        }
    }
}

/// Liegen die (normierten) Normalen parallel oder gegenläufig (innerhalb von `tol.angular`)?
fn parallel_normals(sk: &Sketch, n1: DVec3, n2: DVec3) -> bool {
    n1.cross(n2).length() <= sk.tolerance().angular
}

/// Zeilen „die Punkte `pts` liegen in der Ebene durch `m` mit der Normale `n`“, ohne Zeilen, die
/// nur eine Zahl ohne Variable wären oder eine andere wiederholten: Bei der Normale ±z ist die
/// Zeile eines 2D-Punkts `−m.z`; sie zählt nur einmal und nur, wenn `m` ein 3D-Punkt ist.
fn plane_rows(ctx: &Ctx, pts: &[PointId], m: PointId, n: DVec3) -> Vec<Dual> {
    let has_z = |p: PointId| ctx.sk.points[p].z.is_some();
    let mm = ctx.pt3(m);
    let row = |p: PointId| dot3c(&sub3(&ctx.pt3(p), &mm), n);
    if n.x != 0.0 || n.y != 0.0 {
        return pts.iter().map(|&p| row(p)).collect();
    }
    let mut rows = Vec::new();
    let mut flat_done = false;
    for &p in pts {
        if has_z(p) {
            rows.push(row(p));
        } else if has_z(m) && !flat_done {
            rows.push(row(p));
            flat_done = true;
        }
    }
    rows
}

/// Mitte einer (geprüften) runden Kurve oder Ellipse.
fn center_of(sk: &Sketch, c: CurveId) -> PointId {
    match sk.curves[c].kind {
        CurveKind::Circle { center, .. }
        | CurveKind::Arc { center, .. }
        | CurveKind::Ellipse { center, .. }
        | CurveKind::EllipticArc { center, .. } => center,
        _ => unreachable!("type was checked"),
    }
}

/// Die Punkte einer (geprüften) Linie.
fn line_points(sk: &Sketch, c: CurveId) -> [PointId; 2] {
    let CurveKind::Line { a, b } = sk.curves[c].kind else {
        unreachable!("type was checked")
    };
    [a, b]
}

/// Mit Vorzeichen: Abstand der Mitte `m` von der Geraden durch `a` mit Richtung `d`, gemessen in
/// der Ebene mit der Normale `n` (links der Richtung positiv). In 2D `signed_distance`.
fn signed_distance3(a: &V3, d: &V3, m: &V3, n: DVec3) -> Dual {
    dot3c(&cross3(d, &sub3(m, a)), n) / norm_len3(d)
}

// ---- Residuen -----------------------------------------------------------------------------

/// Residuen der 3D-Fassung; `None` heißt: Die Art rechnet wie in 2D (siehe Modulbeschreibung).
pub(crate) fn eval(ctx: &Ctx, data: &ConstraintData) -> Option<Vec<Dual>> {
    use ConstraintKind::*;
    let sk = ctx.sk;
    Some(match &data.kind {
        Coincident(a, b) => {
            let (p, q) = (ctx.pt3(*a), ctx.pt3(*b));
            vec![&p.0 - &q.0, &p.1 - &q.1, &p.2 - &q.2]
        }
        PointOnCurve(p, c) => {
            let pp = ctx.pt3(*p);
            match &sk.curves[*c].kind {
                CurveKind::Line { .. } => {
                    let (a, d) = line_dir3(ctx, *c);
                    on_line_rows(&a, &d, &pp).to_vec()
                }
                CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
                    let n = sk.curve_normal(*c);
                    let (m, ..) = ellipse_of3(ctx, *c);
                    vec![ellipse_residual3(ctx, *c, &pp), dot3c(&sub3(&pp, &m), n)]
                }
                CurveKind::Spline { .. }
                | CurveKind::ControlSpline { .. }
                | CurveKind::Conic { .. }
                | CurveKind::FixedSpline { .. } => {
                    let t = extra_var(ctx, data);
                    let (q, _) = param_curve3(ctx, *c, &t);
                    let d = sub3(&q, &pp);
                    vec![d.0, d.1, d.2]
                }
                _ => {
                    let (m, r, n) = circle_of3(ctx, *c);
                    let v = sub3(&pp, &m);
                    vec![length3(&v) - r, dot3c(&v, n)]
                }
            }
        }
        Horizontal(l) | Vertical(l) | AlongZ(l) => {
            let (a, b) = match l {
                LineOrPoints::Line(c) => {
                    let (a, d) = line_dir3(ctx, *c);
                    let b = add3(&a, &d);
                    (a, b)
                }
                LineOrPoints::Points(a, b) => (ctx.pt3(*a), ctx.pt3(*b)),
            };
            let d = sub3(&a, &b);
            match data.kind {
                Horizontal(_) => vec![d.1, d.2],
                Vertical(_) => vec![d.0, d.2],
                _ => vec![d.0, d.1],
            }
        }
        Parallel(c1, c2) => {
            let ((_, d1), (_, d2)) = (line_dir3(ctx, *c1), line_dir3(ctx, *c2));
            parallel_rows(&d1, &d2).to_vec()
        }
        Perpendicular(c1, c2) => {
            let ((_, d1), (_, d2)) = (line_dir3(ctx, *c1), line_dir3(ctx, *c2));
            vec![dot3(&d1, &d2) / (norm_len3(&d1) * norm_len3(&d2))]
        }
        Collinear(l1, l2) => {
            let ((a, d1), (_, d2)) = (line_dir3(ctx, *l1), line_dir3(ctx, *l2));
            let Some(q) = collinear_point(sk, *l1, *l2) else {
                debug_assert!(false, "type was checked");
                return Some(Vec::new());
            };
            let mut rows = parallel_rows(&d1, &d2).to_vec();
            // Mit gemeinsamem Ende folgt die Lage aus der Richtung (wie in 2D).
            if !lines_share_point(sk, *l1, *l2) {
                rows.extend(on_line_rows(&a, &d1, &ctx.pt3(q)));
            }
            rows
        }
        Equal(c1, c2) => {
            if is_line_kind(&sk.curves[*c1].kind) {
                let ((_, d1), (_, d2)) = (line_dir3(ctx, *c1), line_dir3(ctx, *c2));
                vec![length3(&d1) - length3(&d2)]
            } else {
                let ((_, r1, _), (_, r2, _)) = (circle_of3(ctx, *c1), circle_of3(ctx, *c2));
                vec![r1 - r2]
            }
        }
        Fixed3(p, x, y, z) => {
            let pp = ctx.pt3(*p);
            vec![&pp.0 - *x, &pp.1 - *y, &pp.2 - *z]
        }
        Midpoint(p, line) => {
            let pp = ctx.pt3(*p);
            let (a, d) = line_dir3(ctx, *line);
            let half = Dual::constant(0.5);
            let m = add3(&a, &scale3(&d, &half));
            let r = sub3(&pp, &m);
            vec![r.0, r.1, r.2]
        }
        Distance(a, b, v) => vec![length3(&sub3(&ctx.pt3(*a), &ctx.pt3(*b))) - *v],
        Radius(c, v) => vec![circle_of3(ctx, *c).1 - *v],
        Diameter(c, v) => vec![2.0 * circle_of3(ctx, *c).1 - *v],
        Angle(c1, c2, v) => {
            // Im Raum hat der Winkel kein Vorzeichen: `sin(θ − |v|)` mit θ ∈ [0, π].
            let ((_, d1), (_, d2)) = (line_dir3(ctx, *c1), line_dir3(ctx, *c2));
            let den = norm_len3(&d1) * norm_len3(&d2);
            let sin = length3(&cross3(&d1, &d2));
            let w = v.abs();
            vec![(sin * w.cos() - dot3(&d1, &d2) * w.sin()) / den]
        }
        Tangent(c1, c2) if line_and_param(sk, *c1, *c2).is_some() => {
            let Some((line, curve)) = line_and_param(sk, *c1, *c2) else {
                unreachable!("checked in the guard")
            };
            let t = extra_var(ctx, data);
            let (q, dq) = param_curve3(ctx, curve, &t);
            let (a, d) = line_dir3(ctx, line);
            if matches!(
                sk.curves[curve].kind,
                CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. }
            ) {
                // Ellipse: eben. Die Linie liegt in ihrer Ebene und berührt in der Ebene (wie in
                // 2D); `q` und `dq` liegen über `InPlane` von selbst darin. Zeilen im Raum hingen
                // sonst von `az` ab und wiederholten `InPlane`.
                let n = sk.curve_normal(curve);
                let den = norm_len3(&dq) * norm_len3(&d);
                let mut rows = vec![
                    dot3c(&cross3(&d, &dq), n) / den,
                    signed_distance3(&a, &d, &q, n),
                ];
                rows.extend(plane_rows(
                    ctx,
                    &line_points(sk, line),
                    center_of(sk, curve),
                    n,
                ));
                return Some(rows);
            }
            // Raumkurve: Richtung parallel (2 Winkelzeilen), Berührpunkt auf der Linie (2 Zeilen).
            let mut rows = parallel_rows(&d, &dq).to_vec();
            rows.extend(on_line_rows(&a, &d, &q));
            rows
        }
        Tangent(c1, c2) if shared_end(sk, *c1, *c2).is_some() => {
            let Some(p) = shared_end(sk, *c1, *c2) else {
                unreachable!("checked in the guard")
            };
            let (sin, off, cos) = shared_end_parts3(ctx, *c1, *c2, p);
            let s = if data.shared_sign < 0 { -1.0 } else { 1.0 };
            let cos = cos * s;
            let mut rows = vec![sin.atan2(&cos)];
            rows.extend(off.map(|o| o.atan2(&cos)));
            rows
        }
        Tangent(c1, c2) => {
            let s = f64::from(data.branch);
            let line_round = |line: CurveId, round: CurveId| {
                let (m, r, n) = circle_of3(ctx, round);
                let (a, d) = line_dir3(ctx, line);
                let mut rows = vec![s * signed_distance3(&a, &d, &m, n) - r];
                rows.extend(plane_rows(
                    ctx,
                    &line_points(sk, line),
                    center_of(sk, round),
                    n,
                ));
                rows
            };
            match (&sk.curves[*c1].kind, &sk.curves[*c2].kind) {
                (CurveKind::Line { .. }, _) => line_round(*c1, *c2),
                (_, CurveKind::Line { .. }) => line_round(*c2, *c1),
                _ => {
                    let ((m1, r1, n1), (m2, r2, _)) = (circle_of3(ctx, *c1), circle_of3(ctx, *c2));
                    let (s1, s2) = match data.branch {
                        1 => (1.0, -1.0),
                        -1 => (-1.0, 1.0),
                        _ => (1.0, 1.0),
                    };
                    let gap = sub3(&m2, &m1);
                    vec![length3(&gap) - (s1 * r1 + s2 * r2), dot3c(&gap, n1)]
                }
            }
        }
        // Gesperrt in 3D (nicht neu anzulegen), aber an gedrehten Ellipsen aus älteren Schritten
        // im Raum gemessen.
        EllipseMajor(c, v) => vec![length3(&ellipse_of3(ctx, *c).1) - *v],
        // Winkel der großen Halbachse in der Draufsicht, mit der Länge im Raum normiert.
        EllipseAngle(c, v) => {
            let (_, a, len, _) = ellipse_of3(ctx, *c);
            vec![(&a.0 * v.sin() - &a.1 * v.cos()) / len]
        }
        _ => return None,
    })
}

/// Welche Zeilen der 3D-Fassung Winkel sind (für `satisfied`).
pub(crate) fn angular_row(sk: &Sketch, kind: &ConstraintKind, row: usize) -> bool {
    use ConstraintKind::*;
    match kind {
        Parallel(..) | Perpendicular(..) | Angle(..) | EllipseAngle(..) => true,
        Collinear(..) => row < 2,
        Tangent(a, b) => match line_and_param(sk, *a, *b) {
            Some((_, c))
                if matches!(
                    sk.curves[c].kind,
                    CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. }
                ) =>
            {
                row == 0
            }
            Some(_) => row < 2,
            None => false,
        },
        _ => false,
    }
}

impl Sketch {
    /// Ist an der Abhängigkeit `kind` 3D-Geometrie beteiligt: ein 3D-Punkt oder eine Kurve mit
    /// eigener Normale (oder einem 3D-Punkt)? Dann rechnet sie in der 3D-Fassung. Unbekannte Ids
    /// zählen nicht.
    pub fn constraint_is_3d(&self, kind: &ConstraintKind) -> bool {
        kind.points()
            .iter()
            .any(|&p| self.points.get(p).is_some_and(|d| d.z.is_some()))
            || kind.curves().iter().any(|&c| self.curve_is_3d_raw(c))
    }

    /// Wie [`Sketch::is_3d_curve`], ohne auf Leben zu prüfen (für den Löser und gelöschte
    /// Einträge).
    fn curve_is_3d_raw(&self, c: CurveId) -> bool {
        let Some(d) = self.curves.get(c) else {
            return false;
        };
        d.normal.is_some()
            || d.kind
                .points()
                .iter()
                .any(|&p| self.points.get(p).is_some_and(|q| q.z.is_some()))
            || matches!(&d.kind, CurveKind::FixedSpline { curve, .. }
                if curve.control_points().iter().any(|q| q.z != 0.0))
    }

    /// Die Skizzenachse, der die Linie `line` im Raum am nächsten liegt (Annahme A3): X wird
    /// „waagerecht“, Y „senkrecht“, Z [`ConstraintKind::AlongZ`]. Gleich nahe Achsen entscheiden
    /// in der Reihenfolge X, Y, Z; eine unbekannte Kurve oder keine Linie ergibt X.
    pub fn nearest_axis(&self, line: CurveId) -> Axis3 {
        let Some(CurveKind::Line { a, b }) = self.curve(line).map(|d| &d.kind) else {
            return Axis3::X;
        };
        let d = (self.pos3(*b) - self.pos3(*a)).abs();
        if d.x >= d.y && d.x >= d.z {
            Axis3::X
        } else if d.y >= d.z {
            Axis3::Y
        } else {
            Axis3::Z
        }
    }

    /// Aktueller Wert eines Maßes an 3D-Geometrie; `None`, wenn die Art wie in 2D misst.
    pub(crate) fn measure3(&self, kind: &ConstraintKind) -> Option<f64> {
        use ConstraintKind::*;
        let ctx = Ctx::collecting(self, &self.vars);
        Some(match kind {
            Distance(a, b, _) => length3(&sub3(&ctx.pt3(*a), &ctx.pt3(*b))).v,
            Radius(c, _) => circle_of3(&ctx, *c).1.v,
            Diameter(c, _) => 2.0 * circle_of3(&ctx, *c).1.v,
            Angle(c1, c2, _) => {
                let ((_, d1), (_, d2)) = (line_dir3(&ctx, *c1), line_dir3(&ctx, *c2));
                length3(&cross3(&d1, &d2)).v.atan2(dot3(&d1, &d2).v)
            }
            EllipseMajor(c, _) => length3(&ellipse_of3(&ctx, *c).1).v,
            _ => return None,
        })
    }

    /// Vorzeichen der Tangente am gemeinsamen Endpunkt `p` im Raum (wie `shared_end_sign`).
    pub(crate) fn shared_end_sign3(&self, a: CurveId, b: CurveId, p: PointId) -> i8 {
        let ctx = Ctx::collecting(self, &self.vars);
        if shared_end_parts3(&ctx, a, b, p).2.v < 0.0 {
            -1
        } else {
            1
        }
    }

    /// Lage einer Tangente ohne gemeinsamen Endpunkt im Raum (wie `initial_branch`); `None`, wenn
    /// die Art keine Lage im Raum braucht.
    pub(crate) fn initial_branch3(&self, kind: &ConstraintKind) -> Option<i8> {
        let ConstraintKind::Tangent(a, b) = *kind else {
            return None;
        };
        if line_and_param(self, a, b).is_some() {
            return Some(0);
        }
        let ctx = Ctx::collecting(self, &self.vars);
        let sign = |v: f64| if v >= 0.0 { 1 } else { -1 };
        let line_round = |line: CurveId, round: CurveId| {
            let (m, _, n) = circle_of3(&ctx, round);
            let (o, d) = line_dir3(&ctx, line);
            sign(signed_distance3(&o, &d, &m, n).v)
        };
        Some(match (&self.curves[a].kind, &self.curves[b].kind) {
            (CurveKind::Line { .. }, _) => line_round(a, b),
            (_, CurveKind::Line { .. }) => line_round(b, a),
            _ => {
                let ((m1, r1, _), (m2, r2, _)) = (circle_of3(&ctx, a), circle_of3(&ctx, b));
                let d = length3(&sub3(&m1, &m2)).v;
                let (r1, r2) = (r1.v, r2.v);
                [
                    (0, (d - (r1 + r2)).abs()),
                    (1, (d - (r1 - r2)).abs()),
                    (-1, (d - (r2 - r1)).abs()),
                ]
                .into_iter()
                .min_by(|x, y| x.1.total_cmp(&y.1))
                .map_or(0, |c| c.0)
            }
        })
    }

    /// Startwert der Zusatzvariablen im Raum (wie `extra_start`): bei Punkt auf Kurve der
    /// nächste Kurvenpunkt, bei der Tangente Linie–Kurve die Stelle mit kleinstem Abstand zur
    /// Linie und zur Linie paralleler Richtung. `None`, wenn die Abhängigkeit keine braucht.
    pub(crate) fn extra_start3(&self, kind: &ConstraintKind) -> Option<f64> {
        let (curve, score): (CurveId, Box<dyn Fn(DVec3, DVec3) -> f64>) = match *kind {
            ConstraintKind::PointOnCurve(p, c) => {
                let at = self.pos3(p);
                (c, Box::new(move |q: DVec3, _| q.distance_squared(at)))
            }
            ConstraintKind::Tangent(a, b) => {
                let (line, curve) = line_and_param(self, a, b)?;
                let CurveKind::Line { a: la, b: lb } = self.curves[line].kind else {
                    return None;
                };
                let o = self.pos3(la);
                let d = self.pos3(lb) - o;
                let u = d.normalize_or_zero();
                let size = d.length();
                (
                    curve,
                    Box::new(move |q: DVec3, dq: DVec3| {
                        let off = (q - o).cross(u).length_squared();
                        let turn = dq.normalize_or_zero().cross(u).length() * size;
                        off + turn * turn
                    }),
                )
            }
            _ => return None,
        };
        let (lo, hi) = match self.curves[curve].kind {
            CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => (0.0, TAU),
            _ => self.curve_domain3(curve)?,
        };
        let pieces = match &self.curves[curve].kind {
            CurveKind::Spline { points, .. } => points.len().saturating_sub(1).max(1),
            CurveKind::ControlSpline { points, .. } => points.len().max(1),
            _ => 4,
        };
        let point = |t: f64| self.curve_point3(curve, t);
        let value = |t: f64| -> f64 {
            let h = (hi - lo) * 1e-6;
            match (point(t), point(t + h), point(t - h)) {
                (Some(q), Some(q1), Some(q0)) => score(q, (q1 - q0) / (2.0 * h)),
                _ => f64::INFINITY,
            }
        };
        let n = pieces * SAMPLES_PER_PIECE;
        let step = (hi - lo) / n as f64;
        let best = (0..=n)
            .map(|k| lo + step * k as f64)
            .min_by(|x, y| value(*x).total_cmp(&value(*y)))?;
        // Dreiteilung um die beste Stelle.
        let (mut a, mut b) = ((best - step).max(lo), (best + step).min(hi));
        for _ in 0..REFINE_STEPS {
            let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
            if value(m1) <= value(m2) {
                b = m2;
            } else {
                a = m1;
            }
        }
        let t = 0.5 * (a + b);
        Some(if value(t) <= value(best) { t } else { best })
    }

    /// Prüft eine neue Abhängigkeit an 3D-Geometrie und gibt die Punkte zurück, die dafür zu
    /// 3D-Punkten werden (Spec 10), samt den Punkten ebener Kurven, die mit ihnen ihre Ebene
    /// bekommen. Ändert nichts.
    ///
    /// - `AlongZ` braucht 3D-Geometrie (an einer 2D-Linie fiele sie zu einem Punkt zusammen),
    ///   `Fixed3` an einem 2D-Punkt nur z = 0.
    /// - Arten außerhalb der Fusion-Liste sind an 3D-Geometrie gesperrt (`sketch-err-not-in-3d`).
    /// - Deckungsgleich, Punkt auf Kurve und Mittelpunkt (auch Konzentrisch, das deckungsgleiche
    ///   Mitten setzt) heben ihre 2D-Punkte an; gesperrt ist das, wenn an einem davon
    ///   2D-only-Abhängigkeiten, ein Muster, Griffe oder ein Text hängen
    ///   (`sketch-err-point-bound-2d`).
    pub(crate) fn check_3d(&self, kind: &ConstraintKind) -> Result<Vec<PointId>, SketchError> {
        use ConstraintKind::*;
        let is_3d = self.constraint_is_3d(kind);
        match kind {
            AlongZ(_) if !is_3d => {
                return Err(SketchError::InvalidInput(tr!("sketch-err-along-z-flat")));
            }
            Fixed3(p, _, _, z) if !self.is_3d_point(*p) && *z != 0.0 => {
                return Err(SketchError::InvalidInput(tr!("sketch-err-z-on-2d-point")));
            }
            _ => {}
        }
        if !is_3d {
            return Ok(Vec::new());
        }
        if !kind.allowed_in_3d() {
            return Err(SketchError::InvalidInput(tr!("sketch-err-not-in-3d")));
        }
        // Zwei runde Kurven ohne gemeinsames Ende berühren sich hier nur in einer gemeinsamen
        // Ebene (Ruling Review Task 3).
        if let Tangent(a, b) = *kind
            && shared_end(self, a, b).is_none()
            && line_and_param(self, a, b).is_none()
            && !is_line_kind(&self.curves[a].kind)
            && !is_line_kind(&self.curves[b].kind)
            && !parallel_normals(self, self.curve_normal(a), self.curve_normal(b))
        {
            return Err(SketchError::InvalidInput(tr!("sketch-err-tangent-planes")));
        }
        let flat = |p: PointId| self.points[p].z.is_none();
        let seeds: Vec<PointId> = match *kind {
            Coincident(a, b) => [a, b].into_iter().filter(|&p| flat(p)).collect(),
            PointOnCurve(p, c) | Midpoint(p, c) if flat(p) && self.curve_is_3d_raw(c) => vec![p],
            _ => Vec::new(),
        };
        if seeds.is_empty() {
            return Ok(seeds);
        }
        let lifted = self.lift_closure(&seeds);
        for &p in &lifted {
            self.check_point_liftable(p)?;
        }
        Ok(lifted)
    }

    /// Die Punkte `seeds` und alle Punkte ebener Kurven ohne Normale, die an ihnen hängen
    /// (Kreis, Bogen, Ellipse, Ellipsenbogen): Ein solcher Bogen bekommt mit einem 3D-Punkt seine
    /// Ebene, und dann sind all seine Punkte 3D (Task 2). Nur 2D-Punkte, ohne Doppelte.
    fn lift_closure(&self, seeds: &[PointId]) -> Vec<PointId> {
        let mut pts: Vec<PointId> = Vec::new();
        for &p in seeds {
            if !pts.contains(&p) {
                pts.push(p);
            }
        }
        let mut i = 0;
        while i < pts.len() {
            let q = pts[i];
            for (_, d) in self.curves() {
                if d.normal.is_some() || !is_planar_kind(&d.kind) {
                    continue;
                }
                let own = d.kind.points();
                if own.contains(&q) {
                    for r in own {
                        if self.points[r].z.is_none() && !pts.contains(&r) {
                            pts.push(r);
                        }
                    }
                }
            }
            i += 1;
        }
        pts
    }

    /// Darf der 2D-Punkt `p` zum 3D-Punkt werden? Nicht, wenn eine Abhängigkeit, die an 3D-
    /// Geometrie gesperrt ist, ihn direkt oder über eine seiner Kurven liest, wenn er zu einem
    /// Muster, einem Spline mit Griffen oder einem Griff gehört, oder wenn ihn ein Text hält.
    fn check_point_liftable(&self, p: PointId) -> Result<(), SketchError> {
        self.check_liftable(&[p])?;
        let bound = || Err(SketchError::InvalidInput(tr!("sketch-err-point-bound-2d")));
        let curves: Vec<CurveId> = self
            .curves()
            .filter(|(_, d)| d.kind.points().contains(&p))
            .map(|(id, _)| id)
            .collect();
        for &c in &curves {
            let handled = matches!(&self.curves[c].kind,
                CurveKind::Spline { handles, .. } if !handles.is_empty());
            if handled || self.is_handle_curve(c) {
                return bound();
            }
        }
        let in_pattern = self.patterns().any(|(_, d)| {
            d.source_points.contains(&p) || d.instances.iter().any(|i| i.points.contains(&p))
        }) || self.pattern_centers().contains(&p);
        if in_pattern {
            return bound();
        }
        let reads = |k: &ConstraintKind| {
            k.points().contains(&p) || k.curves().iter().any(|c| curves.contains(c))
        };
        let blocked = self
            .constraints
            .iter()
            .any(|(_, d)| d.alive && reads(&d.kind) && !d.kind.allowed_in_3d());
        if blocked {
            return bound();
        }
        Ok(())
    }

    /// Hebt die (geprüften) Punkte aus [`Sketch::check_3d`] an: Sie werden 3D-Punkte, ebene
    /// Kurven an ihnen bekommen die Skizzennormale als eigene Normale samt `InPlane`.
    pub(crate) fn apply_lift(&mut self, lifted: &[PointId]) {
        for &p in lifted {
            self.promote_to_3d(p);
        }
        // Ein `Fixed` an einem angehobenen Punkt hält ihn weiter in der Skizzenebene (Ruling R2):
        // Es wird `Fixed3` mit z = 0.
        let pins: Vec<_> = self
            .constraints
            .iter()
            .filter_map(|(id, d)| match d.kind {
                ConstraintKind::Fixed(p, x, y) if d.alive && lifted.contains(&p) => {
                    Some((id, ConstraintKind::Fixed3(p, x, y, 0.0)))
                }
                _ => None,
            })
            .collect();
        for (id, kind) in pins {
            self.constraints[id].kind = kind;
        }
        let planar: Vec<CurveId> = self
            .curves()
            .filter(|(_, d)| {
                d.normal.is_none()
                    && is_planar_kind(&d.kind)
                    && d.kind.points().iter().any(|q| lifted.contains(q))
            })
            .map(|(id, _)| id)
            .collect();
        for c in planar {
            let done = self.set_curve_normal(c, DVec3::Z);
            debug_assert!(done.is_ok(), "checked in check_3d: {done:?}");
        }
    }
}

/// Kurven mit eigener Ebene (Task 2).
fn is_planar_kind(k: &CurveKind) -> bool {
    matches!(
        k,
        CurveKind::Circle { .. }
            | CurveKind::Arc { .. }
            | CurveKind::Ellipse { .. }
            | CurveKind::EllipticArc { .. }
    )
}
