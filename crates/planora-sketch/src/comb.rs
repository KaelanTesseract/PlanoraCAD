//! Krümmungskamm (Skizzen 11a): Einstellungen je Kurve und die Rechnung der Zähne.
//!
//! Der Kamm zeigt an einer gekrümmten Kurve an jeder Stelle die Krümmung als Strecke auf der
//! konvexen Seite (vom Krümmungsmittelpunkt weg). Das Modell hält nur Dichte und Maßstab; die
//! Ansicht zeichnet die Zähne aus [`Sketch::comb_samples`] und [`comb_teeth`].

use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, Sketch};
use planora_i18n::tr;
use planora_math::DVec3;
use std::ops::RangeInclusive;

/// Erlaubte Dichte (Zähne je Kurve).
pub const COMB_DENSITY: RangeInclusive<u32> = 2..=100;
/// Kleinster und größter Maßstab.
pub const COMB_SCALE_MIN: f64 = 0.01;
pub const COMB_SCALE_MAX: f64 = 100.0;
/// Der längste Zahn ist bei Maßstab 1 so viel der Kurvenlänge.
pub const COMB_MAX_FRACTION: f64 = 0.15;
/// Mindestlänge des längsten Zahns in Bildpunkten (die Ansicht rechnet sie in Weltlänge um).
pub const COMB_MIN_PX: f64 = 10.0;

/// Ab diesem Wert von `kappa_max · Kurvenlänge` (Gesamtdrehung in Bogenmaß) gilt eine Kurve als
/// gerade: Rundungsrauschen darf keinen vollen Zahn ergeben.
const STRAIGHT_TURN: f64 = 1e-9;

/// Einstellungen des Krümmungskamms einer Kurve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombSettings {
    /// Anzahl der Zähne, siehe [`COMB_DENSITY`].
    pub density: u32,
    /// Maßstab der Zahnlänge, von [`COMB_SCALE_MIN`] bis [`COMB_SCALE_MAX`].
    pub scale: f64,
}

impl Default for CombSettings {
    fn default() -> Self {
        CombSettings {
            density: 20,
            scale: 1.0,
        }
    }
}

impl CombSettings {
    /// Liegen beide Werte in ihren Bereichen?
    pub fn is_valid(&self) -> bool {
        COMB_DENSITY.contains(&self.density)
            && self.scale.is_finite()
            && (COMB_SCALE_MIN..=COMB_SCALE_MAX).contains(&self.scale)
    }
}

/// Eine Stelle des Kamms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombSample {
    /// Ort auf der Kurve (Skizzenkoordinaten).
    pub at: DVec3,
    /// Einheitsvektor vom Krümmungsmittelpunkt weg (konvexe Seite).
    pub out: DVec3,
    /// Krümmung, nie negativ.
    pub kappa: f64,
}

/// Zähne als (Fuß, Spitze). Der größte Zahn ist `max(COMB_MAX_FRACTION · curve_length,
/// min_world) · scale` lang, Zahn `i` proportional zu `kappa_i / kappa_max`. Eine gerade Kurve
/// (`kappa_max` null) hat Zähne der Länge null.
pub fn comb_teeth(
    samples: &[CombSample],
    curve_length: f64,
    scale: f64,
    min_world: f64,
) -> Vec<(DVec3, DVec3)> {
    let kappa_max = samples.iter().map(|s| s.kappa).fold(0.0, f64::max);
    let flat = !(kappa_max.is_finite() && kappa_max * curve_length > STRAIGHT_TURN);
    let longest = (COMB_MAX_FRACTION * curve_length).max(min_world) * scale;
    samples
        .iter()
        .map(|s| {
            if flat || !longest.is_finite() {
                (s.at, s.at)
            } else {
                (s.at, s.at + s.out * (longest * s.kappa / kappa_max))
            }
        })
        .collect()
}

impl Sketch {
    /// Kann die Kurve `c` einen Krümmungskamm tragen? Bogen, Kreis, Ellipse, Ellipsenbogen, Spline,
    /// Spline mit Kontrollpunkten, Kegelschnitt und feste (projizierte) Splines; Linien nicht.
    pub fn comb_capable(&self, c: CurveId) -> bool {
        self.curve(c)
            .is_some_and(|d| !matches!(d.kind, CurveKind::Line { .. }))
    }

    /// Der Kamm der Kurve `c`, `None` ohne Kamm (oder für unbekannte Kurven).
    pub fn comb(&self, c: CurveId) -> Option<CombSettings> {
        self.curve(c)?.comb
    }

    /// Schaltet den Kamm der Kurve `c` ein (`Some`) oder aus (`None`). Linien und Werte außerhalb
    /// der Bereiche werden abgelehnt. Ändert weder Werte noch den Zwischenspeicher des Lösers.
    pub fn set_comb(&mut self, c: CurveId, comb: Option<CombSettings>) -> Result<(), SketchError> {
        self.check_curve(c)?;
        if !self.comb_capable(c) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-comb-line")));
        }
        if comb.is_some_and(|s| !s.is_valid()) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-comb-range")));
        }
        self.curves[c].comb = comb;
        self.touch_values();
        Ok(())
    }

    /// Länge der Kurve `c` (in 3D entlang der Raumkurve); `None` für unbekannte oder entartete
    /// Kurven.
    pub fn curve_length(&self, c: CurveId) -> Option<f64> {
        let len = if self.is_3d_curve(c) {
            let (lo, hi) = self.curve_domain3(c)?;
            // Linienzug mit vielen Stützstellen, für die Zahnlänge reicht das weit.
            const STEPS: usize = 2048;
            let mut prev = self.curve_point3(c, lo)?;
            let mut sum = 0.0;
            for i in 1..=STEPS {
                let p = self.curve_point3(c, lo + (hi - lo) * i as f64 / STEPS as f64)?;
                sum += p.distance(prev);
                prev = p;
            }
            sum
        } else {
            self.geom(c)?.length()
        };
        len.is_finite().then_some(len)
    }

    /// Die Stellen des Kamms: `density` Stellen gleichmäßig im Parameter, bei offenen Kurven
    /// mit beiden Enden, bei geschlossenen (Kreis, Ellipse) ohne doppelten Endpunkt. `None` für
    /// Kurven, die keinen Kamm tragen, und entartete Kurven.
    pub fn comb_samples(&self, c: CurveId, density: u32) -> Option<Vec<CombSample>> {
        if !self.comb_capable(c) {
            return None;
        }
        let n = density.max(2) as usize;
        let (lo, hi) = self.curve_domain3(c)?;
        let closed = matches!(
            self.curve(c)?.kind,
            CurveKind::Circle { .. } | CurveKind::Ellipse { .. }
        );
        let n3 = self.is_3d_curve(c);
        // Ebene der Kurve: eigene Normale, in der Skizzenebene deren Normale, sonst keine.
        let plane = match self.curve(c)?.normal {
            Some(n) => Some(n),
            None => (!n3).then_some(DVec3::Z),
        };
        let geom = if n3 { None } else { Some(self.geom(c)?) };
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let f = if closed {
                i as f64 / n as f64
            } else {
                i as f64 / (n - 1) as f64
            };
            let t = lo + (hi - lo) * f;
            let jet = match &geom {
                Some(g) => {
                    let (p, d) = g.eval(t);
                    (p.extend(0.0), d.extend(0.0), g.second(t).extend(0.0))
                }
                None => self.curve_jet3(c, t)?,
            };
            out.push(sample_of(jet, plane)?);
        }
        Some(out)
    }
}

/// Eine Stelle aus Ort und Ableitungen. `plane` ist die Normale der Ebene, in der die Kurve liegt
/// (bei einer Kurve in der Skizzenebene `Z`); damit hat auch eine gerade Stelle eine Seite.
fn sample_of((at, d, dd): (DVec3, DVec3, DVec3), plane: Option<DVec3>) -> Option<CombSample> {
    let speed = d.length();
    if !(speed.is_finite() && speed > 0.0 && at.is_finite() && dd.is_finite()) {
        return None;
    }
    let tangent = d / speed;
    // Anteil der zweiten Ableitung quer zur Tangente zeigt zum Krümmungsmittelpunkt.
    let across = dd - tangent * dd.dot(tangent);
    let kappa = across.length() / (speed * speed);
    let kappa = if kappa.is_finite() { kappa } else { 0.0 };
    let out = if across.length() > 0.0 && kappa > 0.0 {
        -(across / across.length())
    } else {
        // gerade Stelle: links der Fahrtrichtung in der Ebene der Kurve
        let n = plane.unwrap_or_else(|| {
            if tangent.z.abs() < 0.9 {
                DVec3::Z
            } else {
                DVec3::X
            }
        });
        n.cross(tangent).normalize_or_zero()
    };
    Some(CombSample { at, out, kappa })
}
