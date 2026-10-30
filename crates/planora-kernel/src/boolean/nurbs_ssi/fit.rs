//! Annäherung der verfolgten Zweige als Freiformkurven (mit Prüfung gegen beide Flächen) und
//! Einordnung von Berührpunkten.

use super::march::Branch;
use super::{Hints, NURBS_FIT_TOL, Probe, Ssi, msg_fit, msg_partial, msg_touch_crossing};
use crate::geometry::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec3, TAU};

/// Schranke, mit der angenähert wird, als Anteil von `NURBS_FIT_TOL` (Reserve für die Prüfung
/// zwischen den Punkten); sie halbiert sich in jeder weiteren Runde.
const FIT_MARGIN: f64 = 0.5;
/// Prüfstellen je Abschnitt zwischen zwei Punkten (Anteile der Sehne) und die dort verlangte
/// Schranke als Anteil von `NURBS_FIT_TOL` (Reserve für die Abweichung zwischen den Prüfstellen).
const VERIFY_STEPS: [f64; 3] = [0.25, 0.5, 0.75];
const VERIFY_FRACTION: f64 = 0.8;
/// Runden „annähern, prüfen, verdichten“ je Stück.
const FIT_ROUNDS: usize = 5;
/// Höchstzahl Punkte eines Stücks (längere Zweige werden geteilt; die Annäherung hat höchstens
/// `MAX_FIT_CONTROL_POINTS` = 512 Kontrollpunkte).
const MAX_FIT_POINTS: usize = 4000;
/// Höchstzahl Halbierungen eines Stücks, wenn die Annäherung an der Kontrollpunktgrenze scheitert.
const MAX_FIT_SPLITS: usize = 8;
/// Ein Stück wird nur geteilt, wenn jede Hälfte mindestens so viele Punkte hat.
const MIN_PIECE_POINTS: usize = 4;
/// Abstände der Proben um einen Berührpunkt (Anteil der Diagonale) und ihre Zahl. Der kleinere
/// Abstand findet schmale gemeinsame Streifen; er ist so groß, dass eine Berührlinie mit
/// Krümmungsradius bis etwa zur Flächengröße deutlich über `tol.linear` abweicht.
const CONTACT_SCALES: [f64; 2] = [0.01, 0.001];
const CONTACT_DIRECTIONS: usize = 8;
/// So viele gültige Proben (Fußpunkte nicht am Rand) braucht die Aussage „liegt ganz an“.
const MIN_CONTACT_PROBES: usize = 3;

impl Ssi<'_> {
    /// Annäherung eines Zweigs: in Stücke zu höchstens `MAX_FIT_POINTS` Punkten geteilt (deren
    /// Enden aufeinanderfallen), jedes geprüft angenähert.
    pub(super) fn fit_branch(&self, br: &Branch) -> Result<Vec<Curve>, String> {
        // Punkte, die (bis auf `same`) auf ihrem Vorgänger liegen, entfallen (etwa ein Randpunkt
        // der Bisektion direkt neben dem Startpunkt); sie gäben der Annäherung eine willkürliche
        // Tangente. Der letzte Punkt bleibt (er ersetzt seinen Vorgänger).
        let (mut pts, mut hints): (Vec<DVec3>, Vec<Hints>) = (Vec::new(), Vec::new());
        for (p, h) in br.pts.iter().zip(&br.hints) {
            match pts.last() {
                Some(q) if q.distance(*p) <= self.same => {
                    if pts.len() > 1 {
                        let k = pts.len() - 1;
                        (pts[k], hints[k]) = (*p, *h);
                    }
                }
                _ => {
                    pts.push(*p);
                    hints.push(*h);
                }
            }
        }
        if pts.len() < 2 {
            return Err(msg_fit());
        }
        let br = Branch {
            pts,
            hints,
            closed: br.closed,
        };
        let br = &br;
        let n = br.pts.len();
        let pieces = n.div_ceil(MAX_FIT_POINTS).max(1);
        let mut out = Vec::new();
        for k in 0..pieces {
            let a = k * (n - 1) / pieces;
            let b = (k + 1) * (n - 1) / pieces;
            out.extend(self.fit_piece(&br.pts[a..=b], &br.hints[a..=b], 0)?);
        }
        Ok(out)
    }

    /// Annähern, an allen Punkten und Sehnenmitten gegen beide Flächen prüfen
    /// (`|F| ≤ NURBS_FIT_TOL`), bei Verstoß dort Kurvenpunkte einfügen und mit halber Schranke
    /// erneut annähern. Scheitert die Annäherung selbst (Kontrollpunktgrenze), wird das Stück
    /// halbiert.
    fn fit_piece(
        &self,
        pts: &[DVec3],
        hints: &[Hints],
        depth: usize,
    ) -> Result<Vec<Curve>, String> {
        let mut pts = pts.to_vec();
        let mut hints = hints.to_vec();
        let mut tol = FIT_MARGIN * NURBS_FIT_TOL;
        for _ in 0..FIT_ROUNDS {
            // Nur ein Fehlschlag der Annäherung selbst (Kontrollpunktgrenze) teilt das Stück;
            // eine misslungene Prüfung verdichtet (siehe unten).
            let Ok(c) = NurbsCurve::fit(&pts, tol) else {
                let m = pts.len() / 2;
                if depth < MAX_FIT_SPLITS && m >= MIN_PIECE_POINTS {
                    let mut out = self.fit_piece(&pts[..=m], &hints[..=m], depth + 1)?;
                    out.extend(self.fit_piece(&pts[m..], &hints[m..], depth + 1)?);
                    return Ok(out);
                }
                return Err(msg_fit());
            };
            let bad = self.verify(&c, &pts, &hints)?;
            if bad.is_empty() {
                return Ok(vec![Curve::nurbs(c)]);
            }
            // Verdichten: Kurvenpunkt in der Mitte jedes beanstandeten Abschnitts.
            for &i in bad.iter().rev() {
                let (a, b) = (pts[i], pts[i + 1]);
                let mid = (a + b) * 0.5;
                let dir = (b - a).normalize_or_zero();
                if dir == DVec3::ZERO {
                    continue;
                }
                if let Some(q) = self.point_in_plane(mid, mid, dir, hints[i], a.distance(b))? {
                    pts.insert(i + 1, q.q);
                    hints.insert(i + 1, q.hints());
                }
            }
            tol *= 0.5;
        }
        // Die Prüfung scheitert trotz Verdichtung (etwa eine Interpolation mit fast so vielen
        // Kontrollpunkten wie Punkten, die zwischen ungleich verteilten Punkten ausschwingt):
        // das Stück halbieren, wie bei der Kontrollpunktgrenze.
        let m = pts.len() / 2;
        if depth < MAX_FIT_SPLITS && m >= MIN_PIECE_POINTS {
            let mut out = self.fit_piece(&pts[..=m], &hints[..=m], depth + 1)?;
            out.extend(self.fit_piece(&pts[m..], &hints[m..], depth + 1)?);
            return Ok(out);
        }
        Err(msg_fit())
    }

    /// Abschnitte `i` (zwischen Punkt `i` und `i + 1`), an deren Anfang oder an einer der
    /// `VERIFY_STEPS` die Kurve (Parameter nach Sehnenlänge) nicht innerhalb
    /// `VERIFY_FRACTION · NURBS_FIT_TOL` auf beiden Flächen liegt.
    fn verify(&self, c: &NurbsCurve, pts: &[DVec3], hints: &[Hints]) -> Result<Vec<usize>, String> {
        let n = pts.len();
        let mut chord = Vec::with_capacity(n);
        let mut s = 0.0;
        chord.push(0.0);
        for w in pts.windows(2) {
            s += w[0].distance(w[1]);
            chord.push(s);
        }
        if !s.is_finite() || s <= 0.0 {
            return Err(msg_fit());
        }
        let (a, b) = c.domain();
        let at = |x: f64| a + (b - a) * (x / s).clamp(0.0, 1.0);
        let ok = |p: DVec3, h: Hints| -> Result<bool, String> {
            Ok(self
                .probe(p, h)?
                .is_some_and(|pr| pr.residual() <= VERIFY_FRACTION * NURBS_FIT_TOL))
        };
        let mut bad = Vec::new();
        for i in 0..n.saturating_sub(1) {
            let start_ok = ok(c.point(at(chord[i])), hints[i])?;
            let mut inner_ok = true;
            for f in VERIFY_STEPS {
                let x = chord[i] + f * (chord[i + 1] - chord[i]);
                let h = if f < 0.5 { hints[i] } else { hints[i + 1] };
                if !ok(c.point(at(x)), h)? {
                    inner_ok = false;
                    break;
                }
            }
            if !(start_ok && inner_ok) {
                if !start_ok && i > 0 && bad.last() != Some(&(i - 1)) {
                    bad.push(i - 1);
                }
                bad.push(i);
            }
        }
        if !ok(c.point(b), hints[n - 1])? && bad.last() != Some(&(n - 2)) {
            bad.push(n - 2);
        }
        Ok(bad)
    }

    /// Einordnung eines Berührpunkts: Proben im Kreis um ihn (auf der Freiformseite) zeigen, ob
    /// die andere Fläche dort ganz anliegt (teilweise zusammenfallend), auf beiden Seiten liegt
    /// (tangentiale Durchdringung) oder nur auf einer (Berührung, `Ok`). Proben, deren Fußpunkt
    /// auf einem offenen Rand liegt (jenseits einer beschränkten Fläche, dort ist das Vorzeichen
    /// des Abstands nicht aussagekräftig), zählen nicht; Berührung nur am Rand ist `Ok`.
    pub(super) fn check_touch(&self, tp: &Probe) -> Result<(), String> {
        let a_is_nurbs = self.a.nurbs.is_some();
        let (side, other, own) = if a_is_nurbs {
            (&self.a, &self.b, tp.ea)
        } else {
            (&self.b, &self.a, tp.eb)
        };
        let (e1, e2) = own.g.any_orthonormal_pair();
        // Zwei Maßstäbe: Ein schmaler gemeinsamer Streifen liegt zwischen den weiten Proben.
        for scale in CONTACT_SCALES {
            let delta = scale * self.diag;
            let (mut pos, mut neg, mut valid, mut on) = (false, false, 0usize, 0usize);
            for k in 0..CONTACT_DIRECTIONS {
                let ang = TAU * k as f64 / CONTACT_DIRECTIONS as f64;
                let x = tp.q + (e1 * ang.cos() + e2 * ang.sin()) * delta;
                let Some(e) = self.eval_side(side, x, own.foot)? else {
                    continue;
                };
                if e.edge {
                    continue;
                }
                let s = side.foot_point(x, &e);
                let Some(o) = self.eval_side(other, s, None)? else {
                    continue;
                };
                if o.edge {
                    continue;
                }
                valid += 1;
                if o.f.abs() <= self.tol.linear {
                    on += 1;
                } else if o.f > 0.0 {
                    pos = true;
                } else {
                    neg = true;
                }
            }
            if valid >= MIN_CONTACT_PROBES && on == valid {
                return Err(msg_partial());
            }
            if pos && neg {
                return Err(msg_touch_crossing());
            }
        }
        Ok(())
    }
}
