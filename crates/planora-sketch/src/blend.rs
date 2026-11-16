//! Übergangskurve (Skizzen 11a, Spec A2; Fusion: „Blend Curve“).
//!
//! Ein Kontrollpunkt-Spline verbindet zwei Kurvenenden: bei G1 Grad 3 mit 4 Kontrollpunkten, bei
//! G2 Grad 5 mit 6. Erster und letzter Kontrollpunkt **sind** die Endpunkte der Quellkurven. An
//! beiden Enden hält bei G1 „Tangente am Ende“ (`EndTangent`) die Richtung, bei G2 „glatt“
//! (`Smooth`) Richtung und Krümmung. Beide Arten sind ein einziges Bézier-Stück (geklemmter
//! Knotenvektor ohne innere Knoten), darum lässt sich die Anfangslage geschlossen angeben.

use crate::constraint::{ConstraintKind, open_curve_ends};
use crate::error::SketchError;
use crate::model::{CurveId, PointId, Sketch};
use planora_i18n::tr;
use planora_math::DVec2;

/// Stetigkeit an den Anschlüssen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendKind {
    /// „Tangential“: gleiche Richtung.
    G1,
    /// „Krümmung“: gleiche Richtung und Krümmung (Vorgabe wie in Fusion).
    #[default]
    G2,
}

impl BlendKind {
    /// Grad des Splines.
    pub fn degree(self) -> u8 {
        match self {
            BlendKind::G1 => 3,
            BlendKind::G2 => 5,
        }
    }
}

/// Ein gewähltes Ende: die Quellkurve und ihr Endpunkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendEnd {
    pub curve: CurveId,
    pub point: PointId,
}

/// Warum eine Kurve keine Quelle einer Übergangskurve sein kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendRefusal {
    /// Ohne zwei Enden: Kreis, Ellipse, geschlossener fester Spline.
    Closed,
    /// 3D-Kurve (Skizzen 10).
    ThreeD,
    /// Erzeugte Kurve eines Textes.
    Text,
    /// Keine (lebende) Kurve.
    NotCurve,
}

impl BlendRefusal {
    /// Die Meldung für den Zeiger.
    pub fn message(self) -> String {
        match self {
            BlendRefusal::Closed => tr!("sketch-blend-closed"),
            BlendRefusal::ThreeD => tr!("sketch-blend-three-d"),
            BlendRefusal::Text => tr!("sketch-blend-text"),
            BlendRefusal::NotCurve => tr!("sketch-blend-not-curve"),
        }
    }
}

/// Unter dieser Länge gilt eine Richtung aus der Kurve als unbestimmt.
const TINY_DIRECTION: f64 = 1e-12;

/// Zahl der Stücke der Vorschau.
const PREVIEW_STEPS: usize = 64;

impl Sketch {
    /// Kann `c` Quelle einer Übergangskurve sein? Quellen sind offene 2D-Kurven mit zwei Enden:
    /// Linie, Bogen, Ellipsenbogen, beide Splinearten, Kegelschnitt und offene feste Splines.
    pub fn blend_source(&self, c: CurveId) -> Result<(), BlendRefusal> {
        let Some(data) = self.curve(c) else {
            return Err(BlendRefusal::NotCurve);
        };
        if self.text_of_curve(c).is_some() {
            return Err(BlendRefusal::Text);
        }
        if self.is_3d_curve(c) {
            return Err(BlendRefusal::ThreeD);
        }
        match open_curve_ends(&data.kind) {
            Some([first, last]) if first != last => Ok(()),
            _ => Err(BlendRefusal::Closed),
        }
    }

    /// Das Ende der Quelle `c`, das `at` am nächsten liegt; `None`, wenn `c` keine Quelle ist.
    pub fn blend_end_near(&self, c: CurveId, at: DVec2) -> Option<BlendEnd> {
        self.blend_source(c).ok()?;
        let [first, last] = open_curve_ends(&self.curve(c)?.kind)?;
        let point = if self.pos(last).distance(at) < self.pos(first).distance(at) {
            last
        } else {
            first
        };
        Some(BlendEnd { curve: c, point })
    }

    /// Die Kontrollpunkte der Anfangslage (vor dem Lösen), von `a` nach `b`. Hebel `h` ist ein
    /// Drittel des Endabstands, `t` die Einheitsrichtung aus dem Ende von der Quelle weg. G1:
    /// `P1 = P0 + h·t_a`, `P2 = P3 + h·t_b`. G2 dazu die Krümmung der Quelle: `P2 = P0 + 2h·t_a +
    /// (5/4)·κ·h²·N_a`, ebenso `P3` von `b` aus. Dabei ist `κ·N` der Krümmungsvektor der Quelle,
    /// gelesen in Richtung von der Quelle über das Ende in die Übergangskurve; für ein
    /// Bézier-Stück Grad 5 gilt am Anfang `κ = (4/5)·(P1 − P0) × (P2 − P1) / |P1 − P0|³`.
    pub fn blend_control_points(
        &self,
        a: BlendEnd,
        b: BlendEnd,
        kind: BlendKind,
    ) -> Result<Vec<DVec2>, SketchError> {
        self.check_blend_ends(a, b)?;
        let (p0, p_end) = (self.pos(a.point), self.pos(b.point));
        let h = p0.distance(p_end) / 3.0;
        let (ta, ka) = self.blend_leaving(a)?;
        let (tb, kb) = self.blend_leaving(b)?;
        Ok(match kind {
            BlendKind::G1 => vec![p0, p0 + h * ta, p_end + h * tb, p_end],
            BlendKind::G2 => {
                // Links von `t` gilt die Krümmung positiv (wie `perp_dot`).
                let second =
                    |p: DVec2, t: DVec2, k: f64| p + 2.0 * h * t + 1.25 * k * h * h * t.perp();
                vec![
                    p0,
                    p0 + h * ta,
                    second(p0, ta, ka),
                    second(p_end, tb, kb),
                    p_end + h * tb,
                    p_end,
                ]
            }
        })
    }

    /// Linienzug der Anfangslage (ohne Lösen), von `a` nach `b`, für die Vorschau des Werkzeugs.
    pub fn blend_preview(
        &self,
        a: BlendEnd,
        b: BlendEnd,
        kind: BlendKind,
    ) -> Result<Vec<DVec2>, SketchError> {
        let control = self.blend_control_points(a, b, kind)?;
        Ok((0..=PREVIEW_STEPS)
            .map(|i| de_casteljau(&control, i as f64 / PREVIEW_STEPS as f64))
            .collect())
    }

    /// Legt die Übergangskurve von `a` nach `b` an und löst. Lässt sich die Skizze danach
    /// schlechter lösen als vorher ([`SolveReport::is_worse_than`](crate::SolveReport::is_worse_than)),
    /// bleibt alles, wie es war, und es kommt `sketch-blend-unsolvable`. Zwei Enden derselben
    /// Kurve sind erlaubt (Spec A2); das Werkzeug lässt sie nach der Klickregel nicht zu (Ruling F10).
    pub fn add_blend_curve(
        &mut self,
        a: BlendEnd,
        b: BlendEnd,
        kind: BlendKind,
    ) -> Result<CurveId, SketchError> {
        let control = self.blend_control_points(a, b, kind)?;
        // Ein Widerspruch an anderer Stelle bleibt, wie er ist: Abgelehnt wird nur, was dieser
        // Befehl schlechter macht.
        let before = self.clone().solve();
        self.atomically(|s| {
            let last = control.len() - 1;
            let mut points = Vec::with_capacity(control.len());
            points.push(a.point);
            for q in &control[1..last] {
                points.push(s.add_point(q.x, q.y)?);
            }
            points.push(b.point);
            let spline = s.add_control_spline(&points, kind.degree())?;
            for end in [a, b] {
                s.add_constraint(match kind {
                    BlendKind::G1 => {
                        ConstraintKind::EndTangent(spline, end.point, end.curve, end.point)
                    }
                    BlendKind::G2 => {
                        ConstraintKind::Smooth(spline, end.point, end.curve, end.point)
                    }
                })?;
            }
            if s.solve().is_worse_than(&before) {
                return Err(SketchError::InvalidInput(tr!("sketch-blend-unsolvable")));
            }
            Ok(spline)
        })
    }

    /// Prüft zwei gewählte Enden: beide Quellen, Punkte wirklich Enden, nicht dasselbe Ende, nicht
    /// aufeinander.
    fn check_blend_ends(&self, a: BlendEnd, b: BlendEnd) -> Result<(), SketchError> {
        for end in [a, b] {
            if let Err(r) = self.blend_source(end.curve) {
                return Err(SketchError::InvalidInput(r.message()));
            }
            let ends = open_curve_ends(&self.curves[end.curve].kind);
            if !ends.is_some_and(|e| e.contains(&end.point)) {
                return Err(SketchError::InvalidInput(tr!("sketch-blend-not-end")));
            }
        }
        if a == b {
            return Err(SketchError::InvalidInput(tr!("sketch-blend-same-end")));
        }
        if a.point == b.point || self.pos(a.point).distance(self.pos(b.point)) <= self.tol.linear {
            return Err(SketchError::InvalidInput(tr!("sketch-blend-ends-touch")));
        }
        Ok(())
    }

    /// Einheitsrichtung aus dem Ende von der Quelle weg und die Krümmung der Quelle in dieser
    /// Laufrichtung (links positiv). Die Krümmung „in die Kurve hinein“ kehrt beim Umdrehen ihr
    /// Vorzeichen.
    fn blend_leaving(&self, end: BlendEnd) -> Result<(DVec2, f64), SketchError> {
        let (into, kappa_into) = self
            .smooth_end_values(end.curve, end.point)
            .ok_or_else(|| SketchError::InvalidInput(tr!("sketch-blend-not-end")))?;
        let len = into.length();
        if !(len > TINY_DIRECTION && kappa_into.is_finite()) {
            return Err(SketchError::Degenerate(tr!("sketch-blend-no-direction")));
        }
        Ok((-into / len, -kappa_into))
    }
}

/// Punkt des Bézier-Stücks mit den Kontrollpunkten `control` beim Parameter `t ∈ [0, 1]`.
fn de_casteljau(control: &[DVec2], t: f64) -> DVec2 {
    let mut q = control.to_vec();
    for k in (1..q.len()).rev() {
        for i in 0..k {
            q[i] = q[i].lerp(q[i + 1], t);
        }
    }
    q[0]
}
