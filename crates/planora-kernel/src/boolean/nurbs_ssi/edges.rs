//! Startpunkte auf dem Rand der Kandidaten.
//!
//! Über einem angenommenen Kandidaten können die Normalen beider Seiten nicht parallel sein:
//! Die Abstandsfunktion der anderen Fläche hat auf dem Flicken keinen kritischen Punkt, jede
//! Komponente ihrer Nullmenge ist ein Bogen mit beiden Enden auf dem Rand des Flickens (keine
//! geschlossene Schleife). Die Nullstellen längs der vier Randkurven sind also Startpunkte aller
//! Bögen im Kandidaten – auch kurzer, die nur eine Ecke oder einen Rand der Fläche anschneiden.
//! Findet sich auf dem Rand keine Nullstelle, ist der Kandidat leer (gegen analytische Flächen
//! sicher: Lipschitz-Ausschluss und Monotonie über Tangenten- und Gradientenkegel).

use super::seeds::{Cone, Node, analytic_cone, excluded_points, patch_cone, separates, tangents};
use super::{Hints, MSG_LOST, PARALLEL_NORMALS, Probe, Side, Ssi, Uv};
use std::cell::Cell;

use planora_math::DVec3;
use planora_math::nurbs::BezierPatch;

/// Größte Teilungstiefe einer Randkurve (relativ 2^-40).
const EDGE_MAX_DEPTH: usize = 40;
/// Gegen eine Freiformfläche (keine sichere Monotonie-Schranke): Stücke, deren Tangenten vom
/// Normalenkegel des anderen Flickens getrennt sind oder deren Kontrollpolygon höchstens dieser
/// Anteil der Kandidatengröße lang ist, gelten als Stück mit höchstens einer Nullstelle
/// (Schätzung, siehe Spec §13).
const EDGE_NURBS_FRACTION: f64 = 1.0 / 32.0;
/// Bisektionsschritte für eine Nullstelle auf der Randkurve.
const EDGE_BISECT_STEPS: usize = 64;
/// Höchstzahl geprüfter Randstücke insgesamt.
const MAX_EDGE_VISITS: usize = 400_000;
/// Rest einer Nullstelle gegen eine Freiformfläche höchstens dieses Vielfache von `tol.linear`
/// (jenseits einer beschränkten Fläche springt das Vorzeichen ohne Nullstelle).
const EDGE_ROOT_FACTOR: f64 = 10.0;

/// Homogener Kontrollpunkt `(P·w, w)`.
type Hom = (DVec3, f64);

/// Randkurve eines Flickens als rationale Bézier-Kurve (homogene Kontrollpunkte `(P·w, w)`),
/// Parameter `t ∈ [0, 1]` linear auf `s.0 … s.1` der Fläche abgebildet; die andere Richtung
/// steht fest auf `fixed`.
#[derive(Clone)]
pub(super) struct Edge {
    h: Vec<Hom>,
    s: (f64, f64),
    along_u: bool,
    fixed: f64,
}

impl Edge {
    /// Linke und rechte Hälfte sowie der Punkt bei `t` (homogen).
    fn casteljau(&self, t: f64) -> (Vec<Hom>, Vec<Hom>, Hom) {
        let n = self.h.len();
        let mut h = self.h.clone();
        let mut left = Vec::with_capacity(n);
        let mut right = vec![(DVec3::ZERO, 0.0); n];
        for r in 0..n {
            left.push(h[0]);
            right[n - 1 - r] = h[n - 1 - r];
            for k in 0..n - 1 - r {
                h[k] = (
                    h[k].0 * (1.0 - t) + h[k + 1].0 * t,
                    h[k].1 * (1.0 - t) + h[k + 1].1 * t,
                );
            }
        }
        (left, right, h[0])
    }

    fn point(&self, t: f64) -> DVec3 {
        let (.., (p, w)) = self.casteljau(t);
        p / w
    }

    fn split(&self) -> [Edge; 2] {
        let (left, right, _) = self.casteljau(0.5);
        let m = 0.5 * (self.s.0 + self.s.1);
        [
            Edge {
                h: left,
                s: (self.s.0, m),
                ..self.clone()
            },
            Edge {
                h: right,
                s: (m, self.s.1),
                ..self.clone()
            },
        ]
    }

    fn control(&self) -> Vec<DVec3> {
        self.h.iter().map(|(p, w)| *p / *w).collect()
    }

    fn uv(&self, t: f64) -> Uv {
        let x = self.s.0 + (self.s.1 - self.s.0) * t;
        if self.along_u {
            (x, self.fixed)
        } else {
            (self.fixed, x)
        }
    }

    /// Richtungen, deren positive Hülle jede Ableitung enthält (polynomial: Nachbardifferenzen,
    /// rational: alle Differenzen `P_l − P_k`, l > k).
    fn tangents(&self) -> Vec<DVec3> {
        let c = self.control();
        let w0 = self.h[0].1;
        let equal = self
            .h
            .iter()
            .all(|(_, w)| (w - w0).abs() <= 1e-12 * w.abs().max(w0.abs()));
        let mut out = Vec::new();
        for k in 0..c.len() {
            for l in k + 1..c.len() {
                if equal && l != k + 1 {
                    continue;
                }
                let d = c[l] - c[k];
                if d.length() > 0.0 {
                    out.push(d);
                }
            }
        }
        out
    }
}

/// Die vier Randkurven eines Flickens.
fn patch_edges(p: &BezierPatch) -> Vec<Edge> {
    let (nu, nv) = (p.degree_u + 1, p.degree_v + 1);
    let hom = |i: usize, j: usize| {
        let k = i * nv + j;
        (p.control[k] * p.weights[k], p.weights[k])
    };
    vec![
        Edge {
            h: (0..nu).map(|i| hom(i, 0)).collect(),
            s: p.u,
            along_u: true,
            fixed: p.v.0,
        },
        Edge {
            h: (0..nu).map(|i| hom(i, nv - 1)).collect(),
            s: p.u,
            along_u: true,
            fixed: p.v.1,
        },
        Edge {
            h: (0..nv).map(|j| hom(0, j)).collect(),
            s: p.v,
            along_u: false,
            fixed: p.u.0,
        },
        Edge {
            h: (0..nv).map(|j| hom(nu - 1, j)).collect(),
            s: p.v,
            along_u: false,
            fixed: p.u.1,
        },
    ]
}

/// Mitte des Parameterbereichs eines Flickens.
fn mid_uv(p: &BezierPatch) -> Uv {
    (0.5 * (p.u.0 + p.u.1), 0.5 * (p.v.0 + p.v.1))
}

impl Ssi<'_> {
    /// Startpunkte eines angenommenen Kandidaten: Nullstellen der jeweils anderen Fläche längs der
    /// Randkurven jedes Freiform-Flickens, von dort mit Newton auf beide Flächen.
    /// Liefert, ob eine noch nicht abgesuchte Randkurve dabei war.
    pub(super) fn edge_seeds(&mut self, n: &Node) -> Result<bool, String> {
        let mut fresh = false;
        let mut roots: Vec<Root> = Vec::new();
        for side_a in [true, false] {
            let (own, other) = if side_a { (&n.a, &n.b) } else { (&n.b, &n.a) };
            let Some(piece) = own else { continue };
            let other_hint = other.as_ref().map(|o| mid_uv(&o.p));
            // Gegen eine Freiformfläche: Normalenkegel ihres Flickens als Schätzung des
            // Gradienten von `F` in seiner Nähe (Monotonie längs der Randkurve).
            let other_side = if side_a { &self.b } else { &self.a };
            let other_cone = match (other, other_side.nurbs) {
                (Some(o), Some(d)) => {
                    patch_cone(d, &o.p, &tangents(&o.p, true), &tangents(&o.p, false))
                }
                _ => None,
            };
            for e in patch_edges(&piece.p) {
                // Jede Randkurve nur einmal (sie gehört zu mehreren Paaren und zwei Nachbarn);
                // ihre Nullstellen hängen nur von der anderen Fläche ab.
                let key = (
                    side_a,
                    e.fixed.to_bits(),
                    e.s.0.to_bits(),
                    e.s.1.to_bits(),
                    u64::from(e.along_u),
                );
                if !self.done_edges.insert(key) {
                    continue;
                }
                fresh = true;
                self.edge_roots(&e, side_a, other_hint, other_cone, &mut roots)?;
            }
        }
        for Root { x, hints, exact } in roots {
            let r = self.refine_seed(x, hints)?;
            match r.found {
                Some(p) if p.q.distance(x) <= self.h_max => self.handle_seed(p)?,
                _ => match self.refine_in_plane(x, hints)? {
                    Some(p) => self.handle_seed(p)?,
                    // Die Nullstelle ist echt (Vorzeichenwechsel), konvergiert aber nicht in
                    // ihrer Nähe: lieber melden als einen Bogen verlieren.
                    None if exact => return Err(MSG_LOST.into()),
                    // Nur eine Annäherung bis auf die Toleranz ohne Vorzeichenwechsel (das
                    // Randstück berührt die andere Fläche): eine Berührung am Rand.
                    None => match self.probe(x, hints)? {
                        Some(p) if p.residual() <= EDGE_ROOT_FACTOR * self.tol.linear => {
                            self.handle_touch(p)?
                        }
                        _ => {}
                    },
                },
            }
        }
        Ok(fresh)
    }

    /// Zweiter Versuch für eine Randnullstelle `x`: Korrektur in der Ebene durch `x` senkrecht
    /// zur geschätzten Kurventangente `g_a × g_b`. Am Rand einer beschränkten Fläche führt der
    /// freie Newton-Schritt oft über den Rand hinaus (dort ist der Abstand zur Fläche nie null);
    /// in der Ebene bleibt der Punkt beim Bogen.
    fn refine_in_plane(&self, x: DVec3, hints: Hints) -> Result<Option<Probe>, String> {
        let Some(pr) = self.probe(x, hints)? else {
            return Ok(None);
        };
        let Some(t) = self.tangent(&pr) else {
            return Ok(None);
        };
        let scale = (pr.residual() / PARALLEL_NORMALS).min(self.h_max);
        Ok(self
            .point_in_plane(x, x, t, pr.hints(), scale)?
            .filter(|p| p.q.distance(x) <= self.h_max))
    }

    /// Nullstellen von `F_other` längs der Randkurve `e` (gehört zur Seite a, wenn `side_a`).
    fn edge_roots(
        &mut self,
        e0: &Edge,
        side_a: bool,
        other_hint: Option<Uv>,
        other_cone: Option<Cone>,
        out: &mut Vec<Root>,
    ) -> Result<(), String> {
        let lin = self.tol.linear;
        let hints_at = |e: &Edge, t: f64| {
            let own = Some(e.uv(t));
            if side_a {
                Hints {
                    a: own,
                    b: other_hint,
                }
            } else {
                Hints {
                    a: other_hint,
                    b: own,
                }
            }
        };
        // Startwert der Fußpunktsuche auf der anderen Fläche: der zuletzt gefundene Fußpunkt.
        let hint = Cell::new(other_hint);
        let mut stack = vec![(e0.clone(), 0usize)];
        while let Some((e, depth)) = stack.pop() {
            self.edge_visits += 1;
            if self.edge_visits > MAX_EDGE_VISITS {
                return Err(format!(
                    "Zu viele Schnittbereiche (mehr als {MAX_EDGE_VISITS} geprüfte Randstücke)"
                ));
            }
            let ctrl = e.control();
            if ctrl.iter().all(|c| c.distance(ctrl[0]) <= 0.0) {
                // zu einem Punkt zusammengefallener Rand (Pol): nur der Punkt selbst
                let other = self.other_side(side_a);
                if let Some(ev) = self.eval_side(other, ctrl[0], other_hint)?
                    && ev.f.abs() <= self.eps
                {
                    push_root(out, ctrl[0], hints_at(&e, 0.5), self.same, true);
                }
                continue;
            }
            let (lo, hi) = ctrl.iter().fold(
                (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                |(l, h), c| (l.min(*c), h.max(*c)),
            );
            let c = (lo + hi) * 0.5;
            let r = 0.5 * (hi - lo).length();
            let Some(fc) = self.f_other(side_a, c, &hint)? else {
                if depth < EDGE_MAX_DEPTH {
                    stack.extend(e.split().into_iter().map(|s| (s, depth + 1)));
                }
                continue;
            };
            // Lipschitz-Ausschluss (der Abstand zur anderen Fläche ist 1-Lipschitz), gegen
            // analytische Flächen zusätzlich über die Kontrollpunkte (konvexe Hülle).
            let other = self.other_side(side_a);
            // Gegen die Kontrollpunkte genügt ein echtes Vorzeichen (Randen, die innerhalb der
            // Toleranz an der anderen Fläche liegen, aber sie nicht schneiden, ergeben keine
            // Nullstelle; Berührungen findet die Kandidatensuche).
            if fc.abs() > r + lin
                || (other.nurbs.is_none() && excluded_points(other.surf, &ctrl, 0.0))
            {
                continue;
            }
            let monotone = match other.nurbs {
                None => analytic_cone(other.surf, lo, hi)
                    .is_some_and(|cone| separates(&e.tangents(), &cone).0),
                Some(_) => {
                    let len: f64 = ctrl.windows(2).map(|w| w[0].distance(w[1])).sum();
                    len <= EDGE_NURBS_FRACTION * self.max_patch.min(self.diag)
                        || other_cone.is_some_and(|cone| separates(&e.tangents(), &cone).0)
                }
            };
            let (p0, p1) = (e.point(0.0), e.point(1.0));
            let (Some(f0), Some(f1)) = (
                self.f_other(side_a, p0, &hint)?,
                self.f_other(side_a, p1, &hint)?,
            ) else {
                continue;
            };
            if !monotone && f0.abs().max(f1.abs()).max(fc.abs()) <= self.eps {
                // Das Randstück liegt auf der anderen Fläche, etwa ein Rand in einer Ebene oder
                // längs einer Berührlinie: ein Startpunkt genügt, die Verfolgung bzw. die
                // Einordnung als Berührung übernimmt den Rest. (Weiter teilen brächte hier keinen
                // Ausschluss.)
                push_root(out, p0, hints_at(&e, 0.0), self.same, true);
                continue;
            }
            if !monotone && depth < EDGE_MAX_DEPTH {
                stack.extend(e.split().into_iter().map(|s| (s, depth + 1)));
                continue;
            }
            if f0.abs() <= self.eps {
                push_root(out, p0, hints_at(&e, 0.0), self.same, true);
            }
            if f1.abs() <= self.eps {
                push_root(out, p1, hints_at(&e, 1.0), self.same, true);
            }
            if f0.abs() > self.eps && f1.abs() > self.eps && (f0 < 0.0) != (f1 < 0.0) {
                // Regula falsi (Illinois): die Nullstelle bleibt eingeschlossen, konvergiert aber
                // weit schneller als Halbieren; Newton auf beide Flächen verfeinert danach.
                let (mut a, mut b, mut fa, mut fb) = (0.0, 1.0, f0, f1);
                let mut side = 0i8;
                let mut t = 0.5;
                let mut fx = None;
                for _ in 0..EDGE_BISECT_STEPS {
                    t = (a * fb - b * fa) / (fb - fa);
                    if !t.is_finite() || t <= a || t >= b {
                        t = 0.5 * (a + b);
                    }
                    let Some(ft) = self.f_other(side_a, e.point(t), &hint)? else {
                        fx = None;
                        break;
                    };
                    fx = Some(ft);
                    if ft.abs() <= self.eps || b - a <= f64::EPSILON {
                        break;
                    }
                    if (ft < 0.0) == (fa < 0.0) {
                        (a, fa) = (t, ft);
                        if side == -1 {
                            fb *= 0.5;
                        }
                        side = -1;
                    } else {
                        (b, fb) = (t, ft);
                        if side == 1 {
                            fa *= 0.5;
                        }
                        side = 1;
                    }
                }
                let x = e.point(t);
                // Gegen Freiformflächen kann der Wechsel ein Vorzeichensprung jenseits des
                // Rands sein; dann ist es keine Nullstelle.
                if fx.is_some_and(|f| f.abs() <= EDGE_ROOT_FACTOR * lin) {
                    push_root(out, x, hints_at(&e, t), self.same, true);
                }
            } else if !monotone && fc.abs() <= EDGE_ROOT_FACTOR * lin {
                // an der Teilungsgrenze, nicht monoton, die Flächen kommen sich hier bis auf die
                // Toleranz nahe: Kandidat für eine Berührung am Rand
                push_root(out, c, hints_at(&e, 0.5), self.same, false);
            }
        }
        Ok(())
    }

    fn other_side(&self, side_a: bool) -> &Side<'_> {
        if side_a { &self.b } else { &self.a }
    }

    fn f_other(
        &self,
        side_a: bool,
        x: DVec3,
        hint: &Cell<Option<Uv>>,
    ) -> Result<Option<f64>, String> {
        let other = self.other_side(side_a);
        let e = self.eval_side(other, x, hint.get())?;
        if let Some(e) = &e
            && e.foot.is_some()
        {
            hint.set(e.foot);
        }
        Ok(e.map(|e| e.f))
    }
}

/// Nullstelle auf einer Randkurve; `exact`: Vorzeichenwechsel oder `|F| ≤ eps` (sonst nur eine
/// Annäherung bis auf die Toleranz an der Teilungsgrenze).
struct Root {
    x: DVec3,
    hints: Hints,
    exact: bool,
}

fn push_root(out: &mut Vec<Root>, x: DVec3, hints: Hints, same: f64, exact: bool) {
    match out.iter_mut().find(|r| r.x.distance(x) <= same) {
        Some(r) => r.exact |= exact,
        None => out.push(Root { x, hints, exact }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_split_and_evaluation_agree() {
        let e = Edge {
            h: vec![
                (DVec3::new(0.0, 0.0, 0.0), 1.0),
                (DVec3::new(1.0, 2.0, 0.0) * 0.5, 0.5),
                (DVec3::new(2.0, 0.0, 1.0), 1.0),
            ],
            s: (2.0, 4.0),
            along_u: true,
            fixed: 7.0,
        };
        let [l, r] = e.split();
        assert!(l.point(0.5).distance(e.point(0.25)) < 1e-14);
        assert!(r.point(0.5).distance(e.point(0.75)) < 1e-14);
        assert_eq!(r.uv(0.0), (3.0, 7.0));
    }
}
