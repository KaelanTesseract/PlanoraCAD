//! Hautfläche (Skinning, Spec §3.2) aus rationalen quadratischen Querschnitten: Kreisbögen
//! (Rundung) oder Strecken (Fase). Quer Grad 2 (`v ∈ [0, 1]`, `v = 0` ist `p0`, `v = 1` ist
//! `p2`); längs (`u` über genau die gegebenen Parameter der Bahn) stückweise kubische
//! Hermite-Interpolation der homogenen Reihen mit gegebenen Ableitungen (Bézier-Stücke mit
//! gemeinsamen Enden, dreifache innere Knoten, stetig differenzierbar). An den Stationen liefert
//! die Fläche die Querschnitte exakt; Reihen, die linear vom Parameter abhängen (Verschiebung,
//! linear wachsender Radius), auch dazwischen (Zylinder, Kegel, Regelfläche exakt).
//!
//! Genauigkeit dazwischen: Je Stück der Länge `h` weicht jede homogene Reihe `f` (`p0`,
//! `w1 · p1`, `p2`, `w1`) höchstens um `h⁴/384 · max|f⁗|` von der wahren Reihe ab (Fehlerformel
//! der kubischen Hermite-Interpolation, Ableitungen exakt vorausgesetzt). Bleibt das Gewicht
//! exakt, ist der Punktfehler höchstens dieser Wert geteilt durch `min(1, w1)` (scharf: im
//! Zufallstest mit Tori bis etwa 0,98 der Schranke ausgeschöpft).
//!
//! Ändert sich das Gewicht längs (Öffnungswinkel ändert sich entlang der Bahn), gilt diese
//! einfache Schranke **nicht**: auch der Nenner ist dann nur genähert, und sein Fehler geht mit
//! dem Punktabstand von der Gewichtsmitte ein. Beobachtet wird weiter ein Fehler der Ordnung
//! `h⁴` (Radius 2 mm, Öffnung linear 30° → 150° über 10 mm: 33 Stationen etwa 1e-7 mm, 5
//! Stationen etwa 4e-4 mm, Verhältnis ≈ 8⁴). Für die Einhaltung einer Toleranz sorgt deshalb
//! der Aufrufer: Stationsdichte wählen und die Fläche gegen exakte Querschnitte nachprüfen
//! (Task 14).

use super::{MAX_CONTROL_POINTS, MIN_WEIGHT, NurbsCurve, NurbsError, NurbsSurface};
use crate::DVec3;
use planora_i18n::tr;
use std::f64::consts::PI;

/// Kleinster Öffnungswinkel eines Querschnittsbogens (wie `MIN_ARC_SWEEP` der Kreisbögen).
const MIN_SECTION_SWEEP: f64 = 1e-9;
/// Sicherheitsfaktor des Zwischengewichts über `MIN_WEIGHT` beim größten Öffnungswinkel.
const SWEEP_WEIGHT_MARGIN: f64 = 2.0;
/// Größter Öffnungswinkel eines einzelnen rationalen quadratischen Stücks (knapp unter π; ein
/// Halbkreis hätte das Gewicht 0). Aus `MIN_WEIGHT` abgeleitet: `w1 = cos(θ/2) = sin((π − θ)/2)`
/// ist an der Grenze `sin(SWEEP_WEIGHT_MARGIN · MIN_WEIGHT) ≈ 2e-6 ≥ MIN_WEIGHT` (für so kleine
/// Winkel ist `sin x = x` bis auf `x³/6`), also lehnt schon `SkinSection::arc` ab, nicht erst
/// `NurbsSurface::new`. Der Zwischenpunkt liegt im Abstand `r / w1` vom Mittelpunkt: nahe der
/// Grenze überschreiten schon Radien ab etwa 1000 mm `MAX_COORD`, das lehnt `skin` benannt ab.
pub const MAX_SECTION_SWEEP: f64 = PI - 2.0 * SWEEP_WEIGHT_MARGIN * MIN_WEIGHT;
/// Relativer Unterschied der Abstände beider Bogenenden vom Mittelpunkt, bis zu dem sie als
/// gleich gelten (Querschnitte aus dem Newton-Verfahren sind auf etwa 1e-12 genau). Bezogen auf
/// `max(Radius, 1 mm)`: unter 1 mm Radius gilt also eine absolute Schranke von 1e-9 mm.
const SAME_RADIUS_REL: f64 = 1e-9;

/// Querschnitt: rationales quadratisches Bézier-Stück `p0 → p2` mit Zwischenpunkt `p1` und
/// dessen Gewicht `w1` (Endgewichte 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinSection {
    pub p0: DVec3,
    pub p1: DVec3,
    pub p2: DVec3,
    pub w1: f64,
}

/// Ableitungen der homogenen Reihen `p0`, `w1 · p1`, `p2` und `w1` nach dem Längsparameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinTangent {
    pub d0: DVec3,
    pub d1: DVec3,
    pub d2: DVec3,
    pub dw: f64,
}

impl SkinSection {
    /// Kreisbogen um `center` von `from` nach `to` (gleicher Abstand, Öffnungswinkel in
    /// `[1e-9, MAX_SECTION_SWEEP]`): Zwischenpunkt im Schnitt der Tangenten, Gewicht `cos(θ/2)`.
    /// Exakter Kreisbogen; Radius null, nicht endliche Zahlen und ungleiche Abstände werden
    /// abgelehnt.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn arc(center: DVec3, from: DVec3, to: DVec3) -> Result<SkinSection, NurbsError> {
        let (a, b) = (from - center, to - center);
        let (ra, rb) = (a.length(), b.length());
        if !(ra.is_finite() && rb.is_finite() && ra > 0.0)
            || (ra - rb).abs() > SAME_RADIUS_REL * ra.max(1.0)
        {
            return Err(NurbsError::Input(tr!("math-arc-radii")));
        }
        // Bei unterlaufendem `ra · rb` ergibt sich NaN und die Bereichsprüfung lehnt ab.
        let theta = (a.dot(b) / (ra * rb)).clamp(-1.0, 1.0).acos();
        if !(theta >= MIN_SECTION_SWEEP) {
            return Err(NurbsError::Input(tr!("math-section-angle-small")));
        }
        if theta > MAX_SECTION_SWEEP {
            return Err(NurbsError::Input(tr!("math-section-angle-180")));
        }
        let w1 = (0.5 * theta).cos();
        let bis = (a / ra + b / rb).normalize();
        let p1 = center + bis * (ra / w1);
        if !p1.is_finite() {
            return Err(NurbsError::Input(tr!("math-section-finite")));
        }
        Ok(SkinSection {
            p0: from,
            p1,
            p2: to,
            w1,
        })
    }

    /// Strecke `from → to` (Fase) als Stück vom Grad 2.
    pub fn line(from: DVec3, to: DVec3) -> SkinSection {
        SkinSection {
            p0: from,
            p1: (from + to) * 0.5,
            p2: to,
            w1: 1.0,
        }
    }

    /// Punkt bei `v ∈ [0, 1]` (begrenzt; nicht endliche `v` ergeben den Anfang `p0`, wie bei
    /// `NurbsSurface::point`).
    pub fn point(&self, v: f64) -> DVec3 {
        let v = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (b0, b1, b2) = ((1.0 - v) * (1.0 - v), 2.0 * v * (1.0 - v) * self.w1, v * v);
        (self.p0 * b0 + self.p1 * b1 + self.p2 * b2) / (b0 + b1 + b2)
    }
}

impl SkinTangent {
    /// Differenzenquotient der homogenen Reihen von `a` nach `b` im Parameterabstand `dx`.
    pub fn between(a: &SkinSection, b: &SkinSection, dx: f64) -> SkinTangent {
        SkinTangent {
            d0: (b.p0 - a.p0) / dx,
            d1: (b.p1 * b.w1 - a.p1 * a.w1) / dx,
            d2: (b.p2 - a.p2) / dx,
            dw: (b.w1 - a.w1) / dx,
        }
    }
}

/// Hautfläche durch die Querschnitte an den streng steigenden Parametern `params` mit den
/// Ableitungen `tangents` (je Station eine; mindestens zwei Stationen, höchstens
/// `(MAX_CONTROL_POINTS − 1)/3 + 1`). Grad 3 × 2, u-Bereich `[params[0], params[n−1]]`,
/// v-Bereich `[0, 1]`. Querschnitte der Länge null (`p0 = p2`), nicht endliche Ableitungen,
/// eine stehende Fläche (alle Querschnitte gleich, alle Ableitungen null, also `∂S/∂u = 0`),
/// nicht positive Gewichte und alles, was `NurbsSurface::new` ablehnt (Knotenabstände, Beträge, nicht endliche Zahlen),
/// ergeben einen benannten Fehler.
pub fn skin(
    sections: &[SkinSection],
    tangents: &[SkinTangent],
    params: &[f64],
) -> Result<NurbsSurface, NurbsError> {
    skin_sided(sections, tangents, tangents, params)
}

/// Wie [`skin`], aber je Station eine Ableitung für das Stück davor (`left`) und eine für das
/// Stück danach (`right`). Sind sie verschieden, ist die Fläche an dieser Station nur stetig
/// (Knick längs), etwa wo der Radius einer Rundung nur stetig ist. Am ersten Querschnitt zählt nur
/// `right`, am letzten nur `left`. Mit `left == right` genau [`skin`].
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn skin_sided(
    sections: &[SkinSection],
    left: &[SkinTangent],
    right: &[SkinTangent],
    params: &[f64],
) -> Result<NurbsSurface, NurbsError> {
    let n = sections.len();
    if n < 2 || left.len() != n || right.len() != n || params.len() != n {
        return Err(NurbsError::Input(tr!("math-sections-mismatch")));
    }
    if params.iter().any(|x| !x.is_finite()) || params.windows(2).any(|w| !(w[1] > w[0])) {
        return Err(NurbsError::Input(tr!("math-params-increasing2")));
    }
    if sections.iter().any(|s| !(s.p0.distance(s.p2) > 0.0)) {
        return Err(NurbsError::Input(tr!("math-section-zero")));
    }
    let finite = |t: &SkinTangent| {
        t.d0.is_finite() && t.d1.is_finite() && t.d2.is_finite() && t.dw.is_finite()
    };
    if !left.iter().chain(right).all(finite) {
        return Err(NurbsError::Input(tr!("math-tangent-finite")));
    }
    let still = |t: &SkinTangent| {
        t.d0 == DVec3::ZERO && t.d1 == DVec3::ZERO && t.d2 == DVec3::ZERO && t.dw == 0.0
    };
    if sections.windows(2).all(|w| w[0] == w[1]) && left.iter().chain(right).all(still) {
        return Err(NurbsError::Input(tr!("math-sections-static")));
    }
    let nu = 3 * (n - 1) + 1;
    if nu > MAX_CONTROL_POINTS {
        return Err(NurbsError::TooLarge(tr!(
            "math-max-sections",
            max = ((MAX_CONTROL_POINTS - 1) / 3 + 1).to_string()
        )));
    }
    let rows = |s: &SkinSection| [(s.p0, 1.0), (s.p1 * s.w1, s.w1), (s.p2, 1.0)];
    let drows = |t: &SkinTangent| [(t.d0, 0.0), (t.d1, t.dw), (t.d2, 0.0)];
    // Homogene Kontrollpunkte, Index `i * 3 + j` wie bei `NurbsSurface` (i längs, j quer).
    let mut hom = vec![(DVec3::ZERO, 0.0); nu * 3];
    for i in 0..n {
        let (r, dl, dr) = (rows(&sections[i]), drows(&left[i]), drows(&right[i]));
        for j in 0..3 {
            hom[3 * i * 3 + j] = r[j];
            if i + 1 < n {
                let h = (params[i + 1] - params[i]) / 3.0;
                hom[(3 * i + 1) * 3 + j] = (r[j].0 + dr[j].0 * h, r[j].1 + dr[j].1 * h);
            }
            if i > 0 {
                let h = (params[i] - params[i - 1]) / 3.0;
                hom[(3 * i - 1) * 3 + j] = (r[j].0 - dl[j].0 * h, r[j].1 - dl[j].1 * h);
            }
        }
    }
    let mut knots = vec![params[0]; 4];
    for &x in &params[1..n - 1] {
        knots.extend([x, x, x]);
    }
    knots.extend([params[n - 1]; 4]);
    // Ein Gewicht ≤ 0 ergibt nicht endliche Punkte oder ein Gewicht unter `MIN_WEIGHT`;
    // beides lehnt `NurbsSurface::new` ab.
    let control = hom.iter().map(|(p, w)| *p / *w).collect();
    let weights = hom.iter().map(|(_, w)| *w).collect();
    NurbsSurface::new(
        3,
        2,
        knots,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        nu,
        3,
        control,
        weights,
    )
}

impl NurbsSurface {
    /// Randkurve `v = v0` (`at_end = false`) bzw. `v = v1`: die erste bzw. letzte
    /// Kontrollpunktreihe mit ihren Gewichten und dem u-Knotenvektor (exakt, weil jeder
    /// Knotenvektor geklemmt ist). Parameter wie `u` der Fläche.
    pub fn v_boundary(&self, at_end: bool) -> Result<NurbsCurve, NurbsError> {
        let (nu, nv) = self.counts();
        let j = if at_end { nv - 1 } else { 0 };
        NurbsCurve::new(
            self.degree_u(),
            self.knots_u().knots().to_vec(),
            (0..nu).map(|i| self.control_at(i, j)).collect(),
            (0..nu).map(|i| self.weight_at(i, j)).collect(),
        )
    }
}
