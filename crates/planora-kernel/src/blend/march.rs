//! Bahn der rollenden Kugel entlang beliebiger Kanten (Spec §3.1, §3.2): Querschnitte an
//! Stationen per Newton (Startwert: voriger Mittelpunkt), Ableitungen über zentrale Differenzen
//! exakter Querschnitte, Hautfläche durch die Querschnitte, Berührkurven als ihre Randkurven.
//!
//! Die Querschnitte liegen in den Ebenen senkrecht zur Kante (Spec §3.1, wie bei den analytischen
//! Rundungen): Kugelmittelpunkt im Abstand `r(s)` von beiden Flächen, Berührpunkte als Fußpunkte.
//! Die Hautfläche ist längs die Hermite-Interpolation dieser exakten Querschnitte (Task 13); ihre
//! Genauigkeit dazwischen ist für ein längs veränderliches Gewicht nicht bewiesen, deshalb wird
//! sie hier **gemessen**: An drei Stellen je Stationsabstand (¼, ½, ¾) wird der exakte Querschnitt
//! neu gerechnet und mit der Hautfläche an fünf Querstellen verglichen (Punkt zu Punkt, also eine
//! Schranke in beide Richtungen). Die Zahl der Stationen verdoppelt sich, bis die Abweichung
//! überall höchstens `NURBS_FIT_TOL / 2` beträgt und die Berührkurven beide Flächen auf
//! `NURBS_EDGE_TOL / 2` treffen; sonst benannte Ablehnung ([`inaccurate`]).
//!
//! Freiformflächen und -kanten (Task 15) laufen denselben Weg mit diesen Unterschieden: Startwert
//! jedes Querschnitts ist die Ebenenformel am Kantenpunkt (der verschobene Nachbarmittelpunkt nur
//! als zweiter Kandidat), Stationen liegen zusätzlich an den inneren Knoten der Kante, und statt
//! gleichmäßig zu verdoppeln wird jedes Stück halbiert, dessen Nachprüfung scheitert
//! ([`march_adaptive`]); die Querschnittsebenen stehen senkrecht auf der Tangente der wahren
//! Schnittkurve beider Flächen (`nA × nB`, [`true_tangent`]) statt auf der einer angepassten
//! Kantenkurve (deren Tangente schwankt und an der Naht geschlossener Kurven einen Knick bis
//! einige 1e-3 rad hat), und die Krümmung der Querschnittsebenen kommt aus diesen Tangenten; an
//! der Naht einseitige Ableitungen mit den wahren Endquerschnitten. Zusätzlich wird
//! jeder Berührpunkt auf einer Freiformfläche als echter Fußpunkt
//! geprüft (auf der Fläche, Mittelpunkt auf seiner Normale; sonst liegt die Kugel jenseits des
//! Flächenstücks: `TooLarge`) und die Krümmung der Freiformfläche zur Kugel hin aus den
//! Fundamentalformen bestimmt (`check_freeform_curvature`, am Kantenpunkt und am Berührpunkt).
//!
//! Geometrische Grenzen werden vor dem Bau geprüft und benannt abgelehnt: Krümmung der
//! Nachbarflächen (`check_curvature`), Querschnitte, die die Drehachse einer Kreiskante bzw. den
//! Krümmungsmittelpunkt der Kante erreichen (die Querschnittsebenen schnitten sich im Bereich der
//! Rundung), stehende Querschnitte (`∂S/∂u` nahe null) und eine Fläche, deren Außenrichtung längs
//! umklappt. Arbeitsgrenzen zählen Stationen und NURBS-Auswertungen, keine Zeit.

use super::analytic::{
    EdgeGeom, SectionKind, VolumeModel, outward_sense, polygon_area_centroid, section_polygon,
    section_seg,
};
use super::chain::ChainedEdge;
use super::error::BlendError;
use super::section::{
    ChamferSpec, Section, Side, chamfer_section, check_curvature, check_freeform_curvature,
    fillet_section,
};
use super::topo::{EdgeSides, face_name, inward_directions};
use crate::boolean::NURBS_FIT_TOL;
use crate::geometry::{Curve, Surface};
use crate::ids::Id;
use crate::topology::{Edge, Solid};
use crate::validate::NURBS_EDGE_TOL;
use planora_i18n::tr;
use planora_math::nurbs::{
    NurbsError, NurbsSurface, SkinSection, SkinTangent, evaluations, skin_sided,
};
use planora_math::{DVec3, RadiusLaw, Tolerance};
use std::cell::Cell;

/// Anfangszahl der gleichmäßigen Stationen (verdoppelt bis `MAX_STATIONS`: 9, 17, 33, 65, 129, 257).
pub const MIN_STATIONS: usize = 9;
/// Höchstzahl der gleichmäßigen Stationen.
pub const MAX_STATIONS: usize = 257;
/// Höchstzahl NURBS-Auswertungen je Kante (deterministisch, wie `MAX_FREEFORM_EVALUATIONS`).
pub const MAX_BLEND_EVALUATIONS: u64 = 20_000_000;
/// Höchstzahl NURBS-Auswertungen einer ganzen Anfrage (Bahnen aller Kanten, Abschlüsse,
/// Prüfungen gegen Löcher und Durchdringungen); geprüft je Querschnitt der Bahn, je Kante, vor
/// jeder Durchdringungsprüfung und vor dem Vernähen (eine einzelne Stufe kann die Grenze um ihren
/// eigenen Aufwand überschreiten). Rein analytische Anfragen werten keine NURBS aus.
pub const MAX_BLEND_REQUEST_EVALUATIONS: u64 = 40_000_000;

thread_local! {
    /// Zählerstand, ab dem die laufende Anfrage abbricht (`None`: keine Anfrage-Grenze).
    static STOP_AT: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Setzt die Anfrage-Grenze für die Dauer des Werts; danach gilt wieder die vorige.
pub(crate) struct BudgetScope {
    previous: Option<u64>,
}

impl BudgetScope {
    /// Grenze von `limit` weiteren NURBS-Auswertungen ab jetzt.
    pub(crate) fn start(limit: u64) -> BudgetScope {
        let previous = STOP_AT.with(|c| c.replace(Some(evaluations().saturating_add(limit))));
        BudgetScope { previous }
    }
}

impl Drop for BudgetScope {
    fn drop(&mut self) {
        STOP_AT.with(|c| c.set(self.previous));
    }
}

/// `TooExpensive`, wenn die laufende Anfrage ihre Grenze überschritten hat.
pub(crate) fn check_budget() -> Result<(), BlendError> {
    match STOP_AT.with(Cell::get) {
        Some(stop) if evaluations() > stop => Err(BlendError::TooExpensive),
        _ => Ok(()),
    }
}
/// Abschnitte der Bogenlängentabelle einer Kante.
const LENGTH_SAMPLES: usize = 512;
/// Gauß-Legendre-Stützstellen auf [−1, 1] und Gewichte (4 Punkte, exakt bis Grad 7).
const GAUSS: [(f64, f64); 4] = [
    (-0.861_136_311_594_052_6, 0.347_854_845_137_453_86),
    (-0.339_981_043_584_856_26, 0.652_145_154_862_546_1),
    (0.339_981_043_584_856_26, 0.652_145_154_862_546_1),
    (0.861_136_311_594_052_6, 0.347_854_845_137_453_86),
];
/// Newton-Schritte beim Umrechnen Bogenlänge → Kantenparameter (Startwert linear im Abschnitt;
/// bei Geraden und Kreisen schon exakt).
const ARC_NEWTON_STEPS: usize = 3;
/// Schritt der zentralen Differenzen relativ zum Stationsabstand (Fehler ~ (Schritt)², Rauschen
/// ~ Newton-Rest / Schritt; beide weit unter der Schranke).
const DERIV_STEP: f64 = 1e-3;
/// Abstand (Bruchteil der Kantenlänge) zwischen Stützstellen des Radiengesetzes und Stationen:
/// Stützstellen näher als das an einem Kantenende entfallen (das Ende ist schon Station); liegt
/// eine innere gleichmäßige Station so nahe an einer Stützstelle, rückt sie genau auf die
/// Stützstelle, sonst wird die Stützstelle als eigene Station eingefügt.
const MIN_STATION_GAP: f64 = 1e-6;
/// Erlaubte Abweichung der Hautfläche vom exakten Querschnitt.
const SKIN_TOL: f64 = 0.5 * NURBS_FIT_TOL;
/// Erlaubter Abstand der Berührkurven von ihren Flächen.
const CONTACT_TOL: f64 = 0.5 * NURBS_EDGE_TOL;
/// Stellen je Stationsabstand (Bruchteile), an denen die Hautfläche nachgeprüft wird.
const CHECK_U: [f64; 3] = [0.25, 0.5, 0.75];
/// Querstellen der Nachprüfung.
const CHECK_V: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
/// Prüfstellen längs bei Freiform-Bahnen (dichter; Review Runde 3: zwischen ¼, ½, ¾ lagen
/// Fasen bis 8,8e-6 neben dem exakten Querschnitt). Enthält ½ (Mitte für die Volumenschätzung).
const CHECK_U_FREE: [f64; 5] = [1.0 / 6.0, 1.0 / 3.0, 0.5, 2.0 / 3.0, 5.0 / 6.0];
/// Querstellen der Nachprüfung bei Freiform-Bahnen.
const CHECK_V_FREE: [f64; 9] = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];
/// Newton-Schritte, mit denen der Kantenpunkt einer Freiform-Fase auf die wahre Schnittkurve
/// beider Flächen (in der Querschnittsebene) gelegt wird.
const CORNER_STEPS: usize = 8;
/// Größter Abstand dieses Punkts vom Kantenpunkt (in Vielfachen von `NURBS_EDGE_TOL`); weiter weg
/// gilt der Kantenpunkt selbst (die Kantenkurve liegt bis `NURBS_EDGE_TOL` auf den Flächen).
const CORNER_REACH: f64 = 10.0;
/// Kleinste Geschwindigkeit der Berührpunkte und der Bogenmitte längs (bezogen auf die
/// Bogenlänge der Kante). Darunter steht der Querschnitt fast still (`∂S/∂u ≈ 0`, entartete
/// Fläche, etwa wenn sich die Querschnittsebenen im Bereich der Rundung schneiden).
const MIN_ROW_SPEED: f64 = 1e-3;
/// Spielraum, mit dem der Kugelmittelpunkt weiter von der Drehachse einer Kreiskante entfernt
/// sein muss als der Radius (wie bei der analytischen Rundung).
const AXIS_MARGIN: f64 = 1e-9;
/// Spielraum der Krümmungsprüfung der Kante: Reichweite des Querschnitts längs der Hauptnormale
/// mal Kantenkrümmung muss darunter bleiben (Querschnittsebenen benachbarter Stellen schneiden
/// sich in der Krümmungsachse, im Abstand des Krümmungsradius).
const EDGE_CURVATURE_LIMIT: f64 = 1.0 - 1e-6;
/// Parameterschritt (Anteil des Kantenbereichs) der numerischen Kantenkrümmung.
const CURVATURE_STEP: f64 = 1e-5;
/// Untergrenze für `1 + nA · nB` beim Verschieben des Startwerts (wie `MIN_BISECTOR` des
/// Querschnitts); darunter bleibt der Startwert unverschoben.
const MIN_BISECTOR_START: f64 = 1e-6;
/// Größter Differenzenschritt als Anteil des Abstands zur Nachbarstation (zwei Schritte der
/// einseitigen Differenz bleiben vor ihr).
const MAX_STEP_GAP: f64 = 0.4;
/// Abstand (Bruchteil der Kantenlänge) der Nachbarstellen, aus deren Flächentangenten die
/// Freiform-Bahn die Krümmung der Querschnittsebenen bestimmt (groß gegen die Anpassungswellen
/// einer angepassten Kantenkurve, klein gegen ihre Gestalt).
const SPINE_STEP: f64 = 1e-3;
/// Größte Abweichung (Sinus) der Flächentangente `nA × nB` von der Tangente der Kantenkurve, bis
/// zu der die Freiform-Bahn die Flächentangente nimmt; darüber (Fußpunktsuche verfehlt, fast
/// tangentiale Flächen) die Kurventangente.
const MAX_TANGENT_SIN: f64 = 1e-2;
/// Höchstzahl der Stationen einer Freiform-Bahn (Knoten der Kante plus angepasste Teilung).
/// Größer als [`MAX_STATIONS`], weil angepasste Kurven (Boolesche Schnittkanten) bis über 200
/// Knoten haben; begrenzt zusätzlich durch [`MAX_BLEND_EVALUATIONS`].
pub const MAX_FREEFORM_STATIONS: usize = 1025;
/// Kleinster Stationsabstand der angepassten Teilung (Bruchteil der Kantenlänge); ein Stück, das
/// darunter noch ungenau ist, ergibt [`inaccurate`].
const MIN_SPAN_FRACTION: f64 = 1e-6;
/// Größter Abstand (in Vielfachen von r) des Kugelmittelpunkts vom Startwert der Ebenenformel,
/// ab dem bei Freiform-Bahnen der zweite Startwert (Nachbarstation) versucht wird.
const MAX_START_JUMP: f64 = 0.5;
/// Meldung, wenn der Querschnittsbogen nicht als ein rationales Stück darstellbar ist (Kanten
/// fast tangential oder sehr spitz; Review M1).
pub fn section_arc() -> String {
    tr!("kernel-blend-section-arc")
}
/// Meldung bei schrägem Abschluss einer Hautfläche (P5).
pub fn oblique_end() -> String {
    tr!("kernel-blend-oblique-end")
}
/// Meldung, wenn die Hautfläche die geforderte Genauigkeit nicht erreicht.
pub fn inaccurate() -> String {
    tr!("kernel-blend-inaccurate")
}
/// Meldung, wenn Querschnitte stillstehen oder die Fläche umklappt.
pub fn folded() -> String {
    tr!("kernel-blend-folded")
}
/// Meldung für Ecken mit variablem Radius (P6, Task 17).
pub fn variable_corner() -> String {
    tr!("kernel-blend-variable-corner")
}

/// Querschnittsgesetz entlang der Kante.
#[derive(Clone, Copy, Debug)]
pub enum SectionLaw<'a> {
    Fillet(f64),
    Variable(&'a RadiusLaw),
    Chamfer(ChamferSpec),
}

/// Kantenparameter zu Bogenlängen: Tabelle mit `LENGTH_SAMPLES` Abschnitten, Längen je Abschnitt
/// per Gauß-Legendre (exakt für Geraden und Kreise), Umkehrung per Newton im Abschnitt.
struct ArcTable<'a> {
    curve: &'a Curve,
    t: Vec<f64>,
    s: Vec<f64>,
}

impl<'a> ArcTable<'a> {
    fn new(e: &'a Edge) -> ArcTable<'a> {
        let mut t = Vec::with_capacity(LENGTH_SAMPLES + 1);
        let mut s = Vec::with_capacity(LENGTH_SAMPLES + 1);
        let mut acc = 0.0;
        let mut table = ArcTable {
            curve: &e.curve,
            t: Vec::new(),
            s: Vec::new(),
        };
        for i in 0..=LENGTH_SAMPLES {
            let ti = e.t0 + (e.t1 - e.t0) * i as f64 / LENGTH_SAMPLES as f64;
            if let Some(&prev) = t.last() {
                acc += table.piece(prev, ti);
            }
            t.push(ti);
            s.push(acc);
        }
        table.t = t;
        table.s = s;
        table
    }

    /// Bogenlänge zwischen den Parametern `a` und `b` (Gauß-Legendre, Betrag).
    fn piece(&self, a: f64, b: f64) -> f64 {
        let (mid, half) = (0.5 * (a + b), 0.5 * (b - a));
        GAUSS
            .iter()
            .map(|(x, w)| w * self.curve.tangent(mid + half * x).length())
            .sum::<f64>()
            * half.abs()
    }

    fn length(&self) -> f64 {
        self.s.last().copied().unwrap_or(0.0)
    }

    /// Bogenlänge ab Kantenanfang beim Kantenparameter `t` (zwischen den Tabellenwerten per
    /// Gauß-Legendre).
    fn arc_at(&self, t: f64) -> f64 {
        let up = self.t.last().copied().unwrap_or(0.0) >= self.t[0];
        let k = self
            .t
            .partition_point(|&v| if up { v < t } else { v > t })
            .clamp(1, self.t.len() - 1);
        self.s[k - 1] + self.piece(self.t[k - 1], t)
    }

    /// Kantenparameter bei der Bogenlänge `x` (begrenzt auf die Kante).
    fn param(&self, x: f64) -> f64 {
        let k = self
            .s
            .partition_point(|&v| v < x)
            .clamp(1, self.s.len() - 1);
        let (s0, s1) = (self.s[k - 1], self.s[k]);
        let (t0, t1) = (self.t[k - 1], self.t[k]);
        let f = if s1 > s0 {
            ((x - s0) / (s1 - s0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut t = t0 + (t1 - t0) * f;
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let sign = (t1 - t0).signum();
        for _ in 0..ARC_NEWTON_STEPS {
            let speed = self.curve.tangent(t).length();
            if !(speed > 0.0 && speed.is_finite()) {
                break;
            }
            let miss = s0 + self.piece(t0, t) - x;
            t = (t - sign * miss / speed).clamp(lo, hi);
        }
        t
    }
}

/// Bogenlänge einer Kante wie in der Bahn (Gauß-Legendre über `LENGTH_SAMPLES` Abschnitte).
pub(crate) fn arc_length(e: &Edge) -> f64 {
    ArcTable::new(e).length()
}

/// Exakter Querschnitt an einer Station.
#[derive(Clone, Copy, Debug)]
struct Station {
    /// Bogenlänge ab Kantenanfang (Parameter der Hautfläche längs).
    x: f64,
    /// Kantenpunkt und Einheitstangente in Kantenrichtung.
    o: DVec3,
    tangent: DVec3,
    sec: Section,
    kind: SectionKind,
}

fn to_skin(st: &Station) -> Result<SkinSection, BlendError> {
    match st.kind {
        SectionKind::Fillet { .. } => SkinSection::arc(st.sec.center, st.sec.pa, st.sec.pb)
            .map_err(|_| BlendError::Unsupported(section_arc())),
        SectionKind::Chamfer(_) => Ok(SkinSection::line(st.sec.pa, st.sec.pb)),
    }
}

/// Einseitige Differenz zweiter Ordnung der homogenen Reihen: `s0` bei `x`, `s1` bei `x + d`,
/// `s2` bei `x + 2d` (`d` mit Vorzeichen), Ableitung `(−3 f₀ + 4 f₁ − f₂) / (2d)`.
fn one_sided(s0: &SkinSection, s1: &SkinSection, s2: &SkinSection, d: f64) -> SkinTangent {
    let f = |a: DVec3, b: DVec3, c: DVec3| (b * 4.0 - a * 3.0 - c) / (2.0 * d);
    SkinTangent {
        d0: f(s0.p0, s1.p0, s2.p0),
        d1: f(s0.p1 * s0.w1, s1.p1 * s1.w1, s2.p1 * s2.w1),
        d2: f(s0.p2, s1.p2, s2.p2),
        dw: (4.0 * s1.w1 - 3.0 * s0.w1 - s2.w1) / (2.0 * d),
    }
}

/// Ableitung mit dem Paar benachbarter Querschnitte und ihrem Abstand (für die Geschwindigkeit).
type Diff = (SkinTangent, SkinSection, SkinSection, f64);

/// Startwert des Kugelmittelpunkts aus der Nachbarstation `prev`: ihr Mittelpunkt, verschoben um
/// die Änderung des Radius längs der Winkelhalbierenden (Ebenenformel
/// `c = o + s·r·(nA + nB)/(1 + nA·nB)` mit den Außennormalen am Kantenpunkt `o`, `s = −1` bei
/// konvexen Kanten (Kugel im Material), `s = +1` bei konkaven). Ohne Rundung oder bei fast
/// entgegengesetzten Normalen unverändert; `fillet_section` legt ihn in die Querschnittsebene
/// und rechnet mit dem vollen Newton-Kriterium.
fn shifted_start(a: Side, b: Side, o: DVec3, prev: &Station, radius: f64, convex: bool) -> DVec3 {
    let SectionKind::Fillet { radius: old } = prev.kind else {
        return prev.sec.center;
    };
    let (na, nb) = (a.normal_at(o), b.normal_at(o));
    let denom = 1.0 + na.dot(nb);
    let s = if convex { -1.0 } else { 1.0 };
    let shift = (na + nb) * (s * (radius - old) / denom);
    if denom > MIN_BISECTOR_START && shift.is_finite() {
        prev.sec.center + shift
    } else {
        prev.sec.center
    }
}

/// Startwert nach der Ebenenformel (siehe [`shifted_start`]) ohne Nachbarstation; `None` bei fast
/// entgegengesetzten Normalen (dann meldet `fillet_section` selbst).
/// `(na, nb)`: Außennormalen beider Flächen am Kantenpunkt `o`.
fn plane_start(na: DVec3, nb: DVec3, o: DVec3, radius: f64, convex: bool) -> Option<DVec3> {
    let denom = 1.0 + na.dot(nb);
    let s = if convex { -1.0 } else { 1.0 };
    let c = o + (na + nb) * (s * radius / denom);
    (denom > MIN_BISECTOR_START && c.is_finite()).then_some(c)
}

/// Innere Knoten einer Freiformkante als Bogenlängen ab Kantenanfang (aufsteigend; bei
/// geschlossenen Kurven auch um eine Periode verschoben, falls der Kantenbereich darüber reicht).
/// Dort ist die Kurve nur endlich oft differenzierbar; Stationen dort halten jedes Stück der
/// Hautfläche innerhalb eines glatten Kurvenstücks.
fn edge_knots(e: &Edge, table: &ArcTable) -> Vec<f64> {
    let Curve::Nurbs(c) = &e.curve else {
        return Vec::new();
    };
    let (lo, hi) = (e.t0.min(e.t1), e.t0.max(e.t1));
    let period = e.curve.period();
    let mut out: Vec<f64> = Vec::new();
    for &k in c.knots().knots() {
        for shift in [-1.0, 0.0, 1.0] {
            let t = k + shift * period.unwrap_or(0.0);
            if t > lo && t < hi && (shift == 0.0 || period.is_some()) {
                out.push(table.arc_at(t));
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

/// Tangente der wahren Schnittkurve beider Flächen aus ihren Normalen `na`, `nb` am Kantenpunkt:
/// `nA × nB` (Richtung wie
/// `curve`). Angepasste Kantenkurven (Boolesche Schnittkanten) liegen nur bis auf die
/// Anpassungsgenauigkeit auf beiden Flächen, ihre Tangente schwankt um bis zu einige 1e-3 rad und
/// hat an der Naht geschlossener Kurven einen Knick; die Querschnittsebenen stehen deshalb
/// senkrecht auf der Flächentangente (glatt, an der Naht stetig). Weicht sie mehr als
/// [`MAX_TANGENT_SIN`] ab, gilt die Kurventangente.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn true_tangent(na: DVec3, nb: DVec3, curve: DVec3) -> DVec3 {
    let t = na.cross(nb).normalize_or_zero();
    let t = if t.dot(curve) < 0.0 { -t } else { t };
    if t != DVec3::ZERO && t.is_finite() && t.cross(curve).length() <= MAX_TANGENT_SIN {
        t
    } else {
        curve
    }
}

/// Wahre Ecke in der Querschnittsebene durch `o` (Normale `t`): Punkt auf beiden Flächen, per
/// Newton auf `(outside_A, outside_B, (p − o)·t) = 0`. Gelingt das nicht oder liegt er weiter als
/// `CORNER_REACH · NURBS_EDGE_TOL` von `o`, bleibt `o`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn true_corner(a: Side, b: Side, o: DVec3, t: DVec3) -> DVec3 {
    let eps = super::section::SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element());
    let mut p = o;
    for _ in 0..CORNER_STEPS {
        let (fa, fb) = (a.outside(p), b.outside(p));
        if !(fa.is_finite() && fb.is_finite()) {
            return o;
        }
        if fa.abs() <= eps && fb.abs() <= eps {
            return if p.distance(o) <= CORNER_REACH * NURBS_EDGE_TOL {
                p
            } else {
                o
            };
        }
        let (na, nb) = (a.normal_at(p), b.normal_at(p));
        let m = planora_math::DMat3::from_cols(na, nb, t).transpose();
        if !(m.determinant().abs() > 0.0) {
            return o;
        }
        let g = (p - o).dot(t);
        p -= m.inverse() * DVec3::new(fa, fb, g);
    }
    o
}

fn is_freeform(s: &Surface) -> bool {
    matches!(s, Surface::Nurbs(_))
}

/// Fußpunkt einer Freiformfläche echt? Der Berührpunkt muss auf der Fläche liegen und der
/// Kugelmittelpunkt auf seiner Normale (seitlicher Versatz höchstens [`CONTACT_TOL`]). Sonst
/// hat die Fußpunktsuche einen Randpunkt der Fläche geliefert (die Kugel liegt jenseits des
/// Flächenstücks, `implicit` springt dort) – die Fläche reicht für das Maß nicht aus.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_foot(side: Side, center: &DVec3, foot: DVec3, face: &str) -> Result<(), BlendError> {
    if !is_freeform(side.surface) {
        return Ok(());
    }
    let n = side.normal_at(foot);
    let lateral = (*center - foot).cross(n).length();
    if !(side.outside(foot).abs() <= CONTACT_TOL && lateral <= CONTACT_TOL) {
        return Err(BlendError::TooLarge { face: face.into() });
    }
    Ok(())
}

fn invalid(err: NurbsError) -> BlendError {
    BlendError::Unsupported(tr!("kernel-blend-surface-invalid", err = err.to_string()))
}

/// Fläche und Schwerpunkt des Querschnitts zwischen Kantenpunkt `o` und Rundung bzw. Fase:
/// Dreieck `o, pa, pb`, bei Rundungen abzüglich des Kreisabschnitts zwischen Sehne und Bogen
/// (der Bogen wölbt sich zur Kante hin, Mittelpunkt und Kantenpunkt liegen auf verschiedenen
/// Seiten der Sehne). Sonst – unerwartete Lage – das Vieleck aus `section_polygon`.
fn section_area(st: &Station) -> (f64, DVec3) {
    let (o, s) = (st.o, &st.sec);
    let tri = 0.5 * (s.pa - o).cross(s.pb - o).length();
    let tri_c = (o + s.pa + s.pb) / 3.0;
    let SectionKind::Fillet { .. } = st.kind else {
        return (tri, tri_c);
    };
    let (a, b) = (s.pa - s.center, s.pb - s.center);
    let r = a.length();
    let theta = (a.dot(b) / (r * b.length())).clamp(-1.0, 1.0).acos();
    let chord_mid = 0.5 * (s.pa + s.pb);
    let opposite = (o - chord_mid).dot(s.center - chord_mid) < 0.0;
    let seg = 0.5 * r * r * (theta - theta.sin());
    let bis = (a + b).normalize_or_zero();
    if !(opposite && seg > 0.0 && seg < tri && bis != DVec3::ZERO) {
        return polygon_area_centroid(&section_polygon(st.kind, o, s));
    }
    // Schwerpunkt des Kreisabschnitts: Abstand 4 r sin³(θ/2) / (3 (θ − sin θ)) vom Mittelpunkt.
    let d = 4.0 * r * (0.5 * theta).sin().powi(3) / (3.0 * (theta - theta.sin()));
    let seg_c = s.center + bis * d;
    let area = tri - seg;
    (area, (tri_c * tri - seg_c * seg) / area)
}

/// Größter Wert von `w · k` über die Einheitsrichtungen `w` vom Kugelmittelpunkt zu den Punkten
/// des Bogens (von `pa` nach `pb`, kürzerer Weg): `|k|` (Anteil in der Bogenebene), wenn dessen
/// Richtung im Bogen liegt, sonst der größere Wert an den Enden. Bei entartetem Bogen der
/// sichere Wert `|k|`.
fn arc_extent(sec: &Section, k: DVec3) -> f64 {
    let (ea, eb) = (
        (sec.pa - sec.center).normalize_or_zero(),
        (sec.pb - sec.center).normalize_or_zero(),
    );
    let n = ea.cross(eb);
    if n == DVec3::ZERO || !n.is_finite() {
        return k.length();
    }
    let n = n.normalize();
    let kp = k - n * k.dot(n);
    let within = ea.cross(kp).dot(n) >= 0.0 && kp.cross(eb).dot(n) >= 0.0;
    if within && kp != DVec3::ZERO {
        kp.length()
    } else {
        ea.dot(k).max(eb.dot(k))
    }
}

/// Kreuzen sich benachbarte Querschnittsebenen im Bereich des Querschnitts? Kreiskanten: Der
/// Kugelmittelpunkt muss weiter von der Drehachse entfernt sein als der Radius (sonst erreicht der
/// Bogen die Achse, in der sich alle Querschnittsebenen schneiden). Andere gekrümmte Kanten:
/// Reichweite des Querschnitts zur Krümmungsachse hin (Abstand längs der Hauptnormale) mal
/// Kantenkrümmung unter [`EDGE_CURVATURE_LIMIT`]. Geraden: nie.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_spine(e: &Edge, t: f64, st: &Station, curvature: Option<DVec3>) -> Result<(), BlendError> {
    let near_axis = || BlendError::Unsupported(tr!("kernel-blend-ball-near-axis"));
    let reach_r = match st.kind {
        SectionKind::Fillet { radius } => radius,
        SectionKind::Chamfer(_) => 0.0,
    };
    match &e.curve {
        Curve::Line { .. } => Ok(()),
        Curve::Circle { frame, .. } => {
            let points = [st.sec.center, st.sec.pa, st.sec.pb];
            let rho = |p: DVec3| {
                let d = p - frame.origin;
                (d - frame.z * d.dot(frame.z)).length()
            };
            let ok = match st.kind {
                SectionKind::Fillet { radius } => rho(st.sec.center) > radius * (1.0 + AXIS_MARGIN),
                SectionKind::Chamfer(_) => points.iter().all(|p| rho(*p) > 0.0),
            };
            if ok { Ok(()) } else { Err(near_axis()) }
        }
        _ => {
            // Die Querschnittsebenen drehen sich um die Krümmungsachse der Kante: Ein Punkt `p`
            // der Ebene bewegt sich längs mit dem Faktor `1 − κ·((p − o)·N)` (N Hauptnormale).
            // Der Querschnitt muss also überall `(p − o)·κN < 1` erfüllen; für die Rundung
            // schätzt `(c − o)·κN + r·κ` den Bogen nach oben ab, für die Fase sind die
            // Berührpunkte die äußersten Punkte (die Strecke ist linear).
            let span = (e.t1 - e.t0).abs();
            let dt = CURVATURE_STEP * span;
            let (lo, hi) = (e.t0.min(e.t1), e.t0.max(e.t1));
            let (ta, tb) = ((t - dt).max(lo), (t + dt).min(hi));
            let kvec = match curvature {
                Some(k) => k,
                None => {
                    let d1 = e.curve.tangent(t);
                    let d2 = (e.curve.tangent(tb) - e.curve.tangent(ta)) / (tb - ta);
                    let speed2 = d1.length_squared();
                    (d2 - d1 * (d2.dot(d1) / speed2)) / speed2
                }
            };
            let reach = match st.kind {
                SectionKind::Fillet { .. } => {
                    (st.sec.center - st.o).dot(kvec) + reach_r * arc_extent(&st.sec, kvec)
                }
                SectionKind::Chamfer(_) => (st.sec.pa - st.o)
                    .dot(kvec)
                    .max((st.sec.pb - st.o).dot(kvec)),
            };
            if !(kvec.is_finite() && reach < EDGE_CURVATURE_LIMIT) {
                return Err(BlendError::Unsupported(tr!(
                    "kernel-blend-too-big-for-curvature"
                )));
            }
            Ok(())
        }
    }
}

/// Querschnitt brauchbar? Nicht endliche Punkte ergeben `SpineLost`, zusammenfallende Punkte
/// eine benannte Ablehnung (wie `analytic::check_section`).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_section(st: &Station, tol: &Tolerance) -> Result<(), BlendError> {
    let s = &st.sec;
    if !(s.center.is_finite() && s.pa.is_finite() && s.pb.is_finite()) {
        return Err(BlendError::SpineLost);
    }
    let min = tol.linear;
    if !(s.pa.distance(st.o) > min && s.pb.distance(st.o) > min && s.pa.distance(s.pb) > min) {
        return Err(BlendError::InvalidParameter(tr!(
            "kernel-blend-contact-too-close",
            mm = planora_i18n::len_arg(min, super::show_num(min)),
            unit = planora_i18n::unit_symbol()
        )));
    }
    Ok(())
}

/// Rundung bzw. Fase entlang der Kante `ce` als Hautfläche. `range = (0, Kantenlänge)` (Bogenlänge
/// ab Kantenanfang); die Berührkurven sind die Randkurven der Hautfläche; die Querschnitte an den
/// Enden sind exakte Kreisbögen bzw. Strecken.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn march_geom(
    solid: &Solid,
    ce: &ChainedEdge,
    sides: &EdgeSides,
    convex: bool,
    law: SectionLaw,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let edge = ce.edge.edge;
    let e = solid.edges.get(edge).ok_or(BlendError::EdgeNotFound)?;
    let (Some(fa), Some(fb)) = (solid.faces.get(sides.a.face), solid.faces.get(sides.b.face))
    else {
        return Err(BlendError::EdgeNotFound);
    };
    let a = Side {
        surface: &fa.surface,
        same_sense: fa.same_sense,
    };
    let b = Side {
        surface: &fb.surface,
        same_sense: fb.same_sense,
    };
    let (name_a, name_b) = (
        face_name(solid, sides.a.face),
        face_name(solid, sides.b.face),
    );
    // Freiformbeteiligung: eine Nachbarfläche ist eine Freiformfläche oder die Kante weder Gerade
    // noch Kreis. Dann gelten zusätzlich die Freiform-Prüfungen (Fußpunkte, Krümmung); rein
    // analytische Bahnen (Task 14) bleiben unverändert.
    let freeform = is_freeform(&fa.surface)
        || is_freeform(&fb.surface)
        || !matches!(e.curve, Curve::Line { .. } | Curve::Circle { .. });
    let table = ArcTable::new(e);
    let length = table.length();
    if !(length > tol.linear && length.is_finite()) {
        return Err(BlendError::SpineLost);
    }
    let closed = e.start == e.end;
    // Bogenlänge auf der Kante → Abstand längs der Kette. Die Kettenlängen (`ChainedEdge`) sind
    // Schätzungen aus Streckenzügen; die Umrechnung staucht deshalb auf `ce.length`, damit der
    // Anteil am Kantenende genau der am Anfang der nächsten Kante ist (glatte Übergänge).
    let scale = if ce.length > 0.0 && ce.length.is_finite() {
        ce.length / length
    } else {
        1.0
    };
    let radius_at = |l: &RadiusLaw, x: f64| l.radius(ce.fraction(x * scale));
    if let SectionLaw::Variable(l) = law
        && closed
        && !((radius_at(l, 0.0) - radius_at(l, length)).abs() <= tol.linear)
    {
        return Err(BlendError::InvalidParameter(tr!(
            "kernel-blend-closed-edge-radius"
        )));
    }
    // Stützstellen des Radiengesetzes auf dieser Kante (Bogenlänge ab Kantenanfang); `kinks` die
    // unter ihnen, an denen das Gesetz nur stetig ist (erster/letzter Punkt innerhalb der Kette).
    let to_edge = |f: f64| {
        let along = f * ce.chain_length - ce.offset;
        (if ce.reversed {
            ce.length - along
        } else {
            along
        }) / scale
    };
    let inside = |x: &f64| *x > MIN_STATION_GAP * length && *x < length * (1.0 - MIN_STATION_GAP);
    let kinks: Vec<f64> = match law {
        SectionLaw::Variable(l) => l.kinks().into_iter().map(to_edge).filter(inside).collect(),
        _ => Vec::new(),
    };
    let breaks: Vec<f64> = match law {
        SectionLaw::Variable(l) => l
            .positions()
            .iter()
            .map(|f| {
                let along = f * ce.chain_length - ce.offset;
                (if ce.reversed {
                    ce.length - along
                } else {
                    along
                }) / scale
            })
            .filter(|x| *x > MIN_STATION_GAP * length && *x < length * (1.0 - MIN_STATION_GAP))
            .collect(),
        _ => Vec::new(),
    };
    let start = evaluations();
    let budget = || {
        if evaluations().saturating_sub(start) > MAX_BLEND_EVALUATIONS {
            Err(BlendError::TooExpensive)
        } else {
            check_budget()
        }
    };
    // Exakter Querschnitt bei der Bogenlänge `x`; `prev` ist eine Nachbarstation (ihr Mittelpunkt,
    // um die Radiusänderung längs der Winkelhalbierenden verschoben, ist der Startwert).
    let section_at = |x: f64, prev: Option<&Station>| -> Result<Station, BlendError> {
        let t = table.param(x);
        let o = e.curve.point(t);
        let curve_tangent = (e.curve.tangent(t) * (e.t1 - e.t0).signum()).normalize_or_zero();
        // Freiform: Normalen am Kantenpunkt einmal (Flächentangente, Startwert, Krümmung).
        let normals = freeform.then(|| (a.normal_at(o), b.normal_at(o)));
        let tangent = match normals {
            Some((na, nb)) => true_tangent(na, nb, curve_tangent),
            None => curve_tangent,
        };
        let kind = match law {
            SectionLaw::Fillet(r) => SectionKind::Fillet { radius: r },
            SectionLaw::Variable(l) => SectionKind::Fillet {
                radius: radius_at(l, x),
            },
            SectionLaw::Chamfer(spec) => SectionKind::Chamfer(spec),
        };
        // Ecke des Querschnitts (Kantenpunkt; bei Freiform-Fasen die wahre Ecke).
        let mut corner_at = o;
        let sec = match kind {
            SectionKind::Fillet { radius } => {
                check_curvature(a, o, convex, radius, &name_a)?;
                check_curvature(b, o, convex, radius, &name_b)?;
                if let Some((na, nb)) = normals {
                    // Krümmung der Freiformflächen schon am Kantenpunkt (zur Kugel hin).
                    let ball = if convex { -1.0 } else { 1.0 };
                    check_freeform_curvature(a, o, na * ball, radius, &name_a)?;
                    check_freeform_curvature(b, o, nb * ball, radius, &name_b)?;
                }
                // Ohne Nachbarstation: bei Freiformbeteiligung die Ebenenformel als Startwert,
                // den `fillet_section` in die Querschnittsebene legt (die numerischen Normalen
                // stehen nicht genau senkrecht zur Kante; Task-2-Hinweis).
                let sec = if freeform {
                    // Freiform (Review I-1): Der verschobene Mittelpunkt der Nachbarstation kann
                    // bei groben Stationen auf der falschen Seite einer engen Krümmung landen,
                    // Newton findet dann eine echte, aber ferne Kugel. Erster Startwert ist
                    // deshalb die Ebenenformel am Kantenpunkt (unabhängig vom Stationsabstand);
                    // der Nachbar-Startwert nur als zweiter Kandidat, wenn der erste weiter als
                    // `MAX_START_JUMP · r` von seinem Startwert wegläuft oder scheitert. Von
                    // mehreren Lösungen gilt die nächste zur Ebenenformel.
                    let ps = normals.and_then(|(na, nb)| plane_start(na, nb, o, radius, convex));
                    let first = fillet_section(a, b, o, tangent, convex, radius, ps);
                    let near = |s: &Section| {
                        ps.is_none_or(|c| s.center.distance(c) <= MAX_START_JUMP * radius)
                    };
                    // Zweiter Startwert: die Nachbarstation, ohne sie der halbe Weg vom
                    // Kantenpunkt zur Ebenenformel (Review: auch die erste Station prüft).
                    let alt = match prev {
                        Some(p) => Some(shifted_start(a, b, o, p, radius, convex)),
                        None => ps.map(|c| o + (c - o) * 0.5),
                    };
                    match (&first, alt) {
                        (Ok(s), _) if near(s) => first?,
                        (_, Some(alt)) => {
                            let second =
                                fillet_section(a, b, o, tangent, convex, radius, Some(alt));
                            match (first, second) {
                                (Ok(x), Ok(y)) => match ps {
                                    Some(c) if y.center.distance(c) < x.center.distance(c) => y,
                                    _ => x,
                                },
                                (Ok(x), Err(_)) => x,
                                (Err(_), Ok(y)) => y,
                                (Err(e1), Err(_)) => return Err(e1),
                            }
                        }
                        (_, None) => first?,
                    }
                } else {
                    let start = prev.map(|p| shifted_start(a, b, o, p, radius, convex));
                    match fillet_section(a, b, o, tangent, convex, radius, start) {
                        // Ein unpassender Startwert (sehr steiles Gesetz) darf die Bahn nicht
                        // abbrechen lassen: noch einmal mit der Ebenenformel am Kantenpunkt.
                        Err(BlendError::SpineLost) if start.is_some() => {
                            fillet_section(a, b, o, tangent, convex, radius, None)?
                        }
                        other => other?,
                    }
                };
                if freeform {
                    check_foot(a, &sec.center, sec.pa, &name_a)?;
                    check_foot(b, &sec.center, sec.pb, &name_b)?;
                    check_freeform_curvature(a, sec.pa, sec.center - sec.pa, radius, &name_a)?;
                    check_freeform_curvature(b, sec.pb, sec.center - sec.pb, radius, &name_b)?;
                }
                sec
            }
            SectionKind::Chamfer(spec) => {
                let (wa, wb) = inward_directions(solid, edge, sides, t);
                // Freiform: Startrichtungen in die Querschnittsebene legen (sie stehen senkrecht
                // auf der Kurventangente, die Ebene auf der Flächentangente; Task-2-Hinweis).
                let (wa, wb) = if freeform {
                    let flat = |w: DVec3| (w - tangent * w.dot(tangent)).normalize_or_zero();
                    (flat(wa), flat(wb))
                } else {
                    (wa, wb)
                };
                // Freiform: Die Fasenmaße gelten ab der wahren Ecke (Schnitt beider Flächen in
                // der Querschnittsebene), nicht ab dem Punkt der angepassten Kantenkurve.
                if freeform {
                    corner_at = true_corner(a, b, o, tangent);
                }
                chamfer_section(a, b, corner_at, tangent, convex, wa, wb, spec)?
            }
        };
        let st = Station {
            x,
            o: corner_at,
            tangent,
            sec,
            kind,
        };
        check_section(&st, tol)?;
        // Freiform: Krümmungsvektor der Querschnittsebenen aus der Flächentangente benachbarter
        // Stellen (Abstand `SPINE_STEP` der Länge); die zweite Ableitung einer angepassten
        // Kantenkurve schwankt mit ihren Anpassungswellen (bis über 10/mm quer zur Kante).
        let curvature = freeform.then(|| {
            let h = SPINE_STEP * length;
            let (xl, xh) = ((x - h).max(0.0), (x + h).min(length));
            let at = |y: f64| {
                let ty = table.param(y);
                let oy = e.curve.point(ty);
                let cy = (e.curve.tangent(ty) * (e.t1 - e.t0).signum()).normalize_or_zero();
                true_tangent(a.normal_at(oy), b.normal_at(oy), cy)
            };
            let d = (at(xh) - at(xl)) / (xh - xl);
            d - tangent * d.dot(tangent)
        });
        check_spine(e, t, &st, curvature)?;
        budget()?;
        Ok(st)
    };
    // Ableitungen der Station `i`, links (für das Stück davor) und rechts (danach), mit den
    // Differenzenschritten `step_l`/`step_r` und dem Schritt `seam` über die Naht. Glatte innere
    // Stationen: zentrale Differenz. Kantenenden und Knicke des Gesetzes: einseitige Differenzen
    // zweiter Ordnung je Seite (am Knick verschieden, die Fläche ist dort nur stetig). Naht einer
    // geschlossenen Kante: zentral über die Naht, außer beim variablen Radius (Steigung am
    // Kettenanfang und -ende darf verschieden sein, dann je Seite einseitig). Die Schritte bleiben
    // unter `MAX_STEP_GAP` des Abstands zur Nachbarstation, so überspannt keine Differenz einen
    // Knick (Knicke sind Stationen).
    let seam_central = closed && !matches!(law, SectionLaw::Variable(_));
    let derive = |stations: &[Station],
                  sk: &[SkinSection],
                  i: usize,
                  step_l: f64,
                  step_r: f64,
                  seam: f64,
                  free: bool|
     -> Result<(SkinTangent, SkinTangent), BlendError> {
        let st = &stations[i];
        let m = stations.len();
        let at = |x: f64| -> Result<SkinSection, BlendError> { to_skin(&section_at(x, Some(st))?) };
        let (first, last) = (i == 0, i == m - 1);
        let kink = kinks.contains(&st.x);
        let central = |step: f64| -> Result<Diff, BlendError> {
            let (sl, sh) = (at(st.x - step)?, at(st.x + step)?);
            Ok((
                SkinTangent::between(&sl, &sh, 2.0 * step),
                sl,
                sh,
                2.0 * step,
            ))
        };
        let side = |dir: f64, step: f64| -> Result<Diff, BlendError> {
            // Freiform, Naht am Kantenende: der wahre Endquerschnitt (die Hautfläche übernimmt
            // dort den Anfangsquerschnitt; beide unterscheiden sich um den kleinen Knick der
            // angepassten Kurve an ihrer Naht).
            let s0 = if free && closed && last {
                to_skin(&section_at(length, Some(st))?)?
            } else {
                sk[i]
            };
            let (s1, s2) = (at(st.x + dir * step)?, at(st.x + 2.0 * dir * step)?);
            Ok((one_sided(&s0, &s1, &s2, dir * step), s0, s1, step))
        };
        let pair = if (first || last) && seam_central && !free {
            let (sl, sh) = (at(length - seam)?, at(seam)?);
            let t = (
                SkinTangent::between(&sl, &sh, 2.0 * seam),
                sl,
                sh,
                2.0 * seam,
            );
            (t, t)
        } else if first {
            let r = side(1.0, step_r)?;
            (r, r)
        } else if last {
            let l = side(-1.0, step_l)?;
            (l, l)
        } else if kink {
            (side(-1.0, step_l)?, side(1.0, step_r)?)
        } else {
            let c = central(step_l.min(step_r))?;
            (c, c)
        };
        // Stehende Querschnitte (∂S/∂u ≈ 0): Berührpunkte und Bogenmitte müssen sich längs
        // bewegen.
        for (_, p, q, span) in [&pair.0, &pair.1] {
            let speed = |u: DVec3, v: DVec3| u.distance(v) / span;
            let slow = [
                speed(p.p0, q.p0),
                speed(p.point(0.5), q.point(0.5)),
                speed(p.p2, q.p2),
            ]
            .iter()
            .any(|v| !(*v >= MIN_ROW_SPEED));
            if slow {
                return Err(BlendError::Unsupported(folded()));
            }
        }
        Ok((pair.0.0, pair.1.0))
    };
    // Nachprüfung eines Stücks gegen exakte Querschnitte: `Some(Mitte)`, wenn es genau genug ist
    // (die Mitte dient der Volumenschätzung), sonst `None`.
    let check_span = |surface: &NurbsSurface,
                      w: &[Station],
                      from: &Station|
     -> Result<Option<Station>, BlendError> {
        let mut mid = None;
        let (check_u, check_v): (&[f64], &[f64]) = if freeform {
            (&CHECK_U_FREE, &CHECK_V_FREE)
        } else {
            (&CHECK_U, &CHECK_V)
        };
        for &f in check_u {
            let xu = w[0].x + (w[1].x - w[0].x) * f;
            let exact = section_at(xu, Some(from))?;
            let want = to_skin(&exact)?;
            let off = check_v
                .iter()
                .any(|&v| !(surface.point(xu, v).distance(want.point(v)) <= SKIN_TOL));
            let loose = !(a.outside(surface.point(xu, 0.0)).abs() <= CONTACT_TOL
                && b.outside(surface.point(xu, 1.0)).abs() <= CONTACT_TOL);
            budget()?;
            if off || loose {
                return Ok(None);
            }
            if f == 0.5 {
                mid = Some(exact);
            }
        }
        Ok(mid)
    };
    let skin_of = |sk: &[SkinSection],
                   lefts: &[SkinTangent],
                   rights: &[SkinTangent],
                   xs: &[f64]|
     -> Result<NurbsSurface, BlendError> {
        skin_sided(sk, lefts, rights, xs).map_err(|err| {
            BlendError::Unsupported(tr!(
                "kernel-blend-surface-cannot-form",
                err = err.to_string()
            ))
        })
    };
    if freeform {
        return march_adaptive(Adaptive {
            length,
            closed,
            breaks: &breaks,
            knots: edge_knots(e, &table),
            section_at: &section_at,
            derive: &derive,
            check_span: &check_span,
            skin_of: &skin_of,
        })
        .and_then(|(surface, stations, mids)| {
            finish(
                edge, sides, convex, closed, length, surface, &stations, &mids,
            )
        });
    }
    let mut n = MIN_STATIONS;
    loop {
        let mut xs: Vec<f64> = (0..n).map(|i| length * i as f64 / (n - 1) as f64).collect();
        for &x in &breaks {
            // Liegt eine Station fast auf der Stützstelle, rückt sie genau dorthin (Knicke des
            // Gesetzes müssen Stationen sein); die Enden bleiben.
            match xs[1..n - 1]
                .iter()
                .position(|s| (s - x).abs() <= MIN_STATION_GAP * length && !breaks.contains(s))
            {
                Some(k) => xs[k + 1] = x,
                None => xs.push(x),
            }
        }
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let m = xs.len();
        let mut stations: Vec<Station> = Vec::with_capacity(m);
        for &x in &xs {
            let s = section_at(x, stations.last())?;
            stations.push(s);
        }
        if closed {
            // Naht: erster und letzter Querschnitt exakt gleich (geschlossene Hautfläche).
            stations[m - 1] = Station {
                x: length,
                ..stations[0]
            };
        }
        let sk = stations
            .iter()
            .map(to_skin)
            .collect::<Result<Vec<_>, _>>()?;
        let delta = DERIV_STEP * length / (n - 1) as f64;
        let mut lefts = Vec::with_capacity(m);
        let mut rights = Vec::with_capacity(m);
        for (i, st) in stations.iter().enumerate() {
            let (first, last) = (i == 0, i == m - 1);
            let gap_l = if first {
                f64::INFINITY
            } else {
                st.x - xs[i - 1]
            };
            let gap_r = if last {
                f64::INFINITY
            } else {
                xs[i + 1] - st.x
            };
            let step_l = delta.min(MAX_STEP_GAP * gap_l);
            let step_r = delta.min(MAX_STEP_GAP * gap_r);
            let (l, r) = derive(&stations, &sk, i, step_l, step_r, delta, false)?;
            lefts.push(l);
            rights.push(r);
        }
        let surface = skin_of(&sk, &lefts, &rights, &xs)?;
        // Nachprüfung gegen exakte Querschnitte; die Mitten dienen auch der Volumenschätzung.
        let mut mids: Vec<Station> = Vec::with_capacity(m - 1);
        let mut accurate = true;
        for (i, w) in stations.windows(2).enumerate() {
            match check_span(&surface, w, &stations[i])? {
                Some(mid) => mids.push(mid),
                None => {
                    accurate = false;
                    break;
                }
            }
        }
        if accurate {
            return finish(
                edge, sides, convex, closed, length, surface, &stations, &mids,
            );
        }
        if n >= MAX_STATIONS {
            return Err(BlendError::Unsupported(inaccurate()));
        }
        n = 2 * n - 1;
    }
}

/// Zwischenstand der Bahn für [`march_adaptive`]: Kantenlänge, Naht, Stützstellen des Gesetzes,
/// Knoten der Kante und die Schritte der Bahn (Querschnitt, Ableitungen, Nachprüfung, Haut).
struct Adaptive<'a> {
    length: f64,
    closed: bool,
    breaks: &'a [f64],
    knots: Vec<f64>,
    #[allow(clippy::type_complexity)]
    section_at: &'a dyn Fn(f64, Option<&Station>) -> Result<Station, BlendError>,
    #[allow(clippy::type_complexity)]
    derive: &'a dyn Fn(
        &[Station],
        &[SkinSection],
        usize,
        f64,
        f64,
        f64,
        bool,
    ) -> Result<(SkinTangent, SkinTangent), BlendError>,
    #[allow(clippy::type_complexity)]
    check_span:
        &'a dyn Fn(&NurbsSurface, &[Station], &Station) -> Result<Option<Station>, BlendError>,
    #[allow(clippy::type_complexity)]
    skin_of: &'a dyn Fn(
        &[SkinSection],
        &[SkinTangent],
        &[SkinTangent],
        &[f64],
    ) -> Result<NurbsSurface, BlendError>,
}

/// Bitmuster einer Ableitung (Schlüssel der Nachprüfungs-Ablage).
fn tangent_bits(t: &SkinTangent, out: &mut Vec<u64>) {
    for v in [t.d0, t.d1, t.d2] {
        out.extend(v.to_array().map(f64::to_bits));
    }
    out.push(t.dw.to_bits());
}

/// Freiform-Bahn mit angepasster Teilung (Review I-2): Anfangsstationen gleichmäßig
/// ([`MIN_STATIONS`]) plus Stützstellen des Gesetzes plus innere Knoten der Kante; danach wird
/// jedes Stück, dessen Nachprüfung gegen exakte Querschnitte scheitert, halbiert (alle solchen
/// Stücke je Durchgang), bis alles genau genug ist. Grenzen: [`MAX_FREEFORM_STATIONS`], kleinstes
/// Stück [`MIN_SPAN_FRACTION`] der Kantenlänge, Auswertungsgrenzen der Bahn; sonst
/// [`inaccurate`]. Querschnitte, Ableitungen und Prüfergebnisse unveränderter Stücke werden
/// wiederverwendet (Schlüssel: Lage, Nachbarabstände bzw. Werte und Ableitungen der Enden), das
/// Ergebnis hängt nur von den Eingaben ab. Die Differenzenschritte sind `DERIV_STEP` mal dem
/// Abstand zur Nachbarstation (Rauschen des Newton-Rests / Schritt mal Stücklänge bleibt so
/// unabhängig von der Teilung).
fn march_adaptive(p: Adaptive) -> Result<(NurbsSurface, Vec<Station>, Vec<Station>), BlendError> {
    use std::collections::HashMap;
    let length = p.length;
    let inside = |x: f64| x > MIN_STATION_GAP * length && x < length * (1.0 - MIN_STATION_GAP);
    let mut xs: Vec<f64> = (0..MIN_STATIONS)
        .map(|i| length * i as f64 / (MIN_STATIONS - 1) as f64)
        .collect();
    xs.extend(p.breaks.iter().copied().filter(|x| inside(*x)));
    xs.extend(p.knots.iter().copied().filter(|x| inside(*x)));
    xs.sort_by(f64::total_cmp);
    // Fast zusammenfallende Stellen zusammenlegen; Stützstellen des Gesetzes haben Vorrang (sie
    // müssen Stationen sein), die Enden bleiben.
    let mut merged: Vec<f64> = Vec::with_capacity(xs.len());
    for x in xs {
        match merged.last_mut() {
            Some(last) if x - *last <= MIN_STATION_GAP * length => {
                let end = *last == 0.0 || x == length;
                if x == length {
                    *last = length;
                } else if !end && p.breaks.contains(&x) {
                    *last = x;
                }
            }
            _ => merged.push(x),
        }
    }
    let mut xs = merged;
    if xs.len() > MAX_FREEFORM_STATIONS {
        return Err(BlendError::Unsupported(inaccurate()));
    }
    let mut sections: HashMap<u64, Station> = HashMap::new();
    let mut diffs: HashMap<[u64; 4], (SkinTangent, SkinTangent)> = HashMap::new();
    let mut spans: HashMap<Vec<u64>, Option<Station>> = HashMap::new();
    loop {
        let m = xs.len();
        let mut stations: Vec<Station> = Vec::with_capacity(m);
        for &x in &xs {
            let st = match sections.get(&x.to_bits()) {
                Some(s) => *s,
                None => {
                    let s = (p.section_at)(x, stations.last())?;
                    sections.insert(x.to_bits(), s);
                    s
                }
            };
            stations.push(st);
        }
        if p.closed {
            stations[m - 1] = Station {
                x: length,
                ..stations[0]
            };
        }
        let sk = stations
            .iter()
            .map(to_skin)
            .collect::<Result<Vec<_>, _>>()?;
        let seam_gap = (xs[1] - xs[0]).min(xs[m - 1] - xs[m - 2]);
        let mut lefts = Vec::with_capacity(m);
        let mut rights = Vec::with_capacity(m);
        for i in 0..m {
            let gap_l = if i == 0 {
                f64::INFINITY
            } else {
                xs[i] - xs[i - 1]
            };
            let gap_r = if i == m - 1 {
                f64::INFINITY
            } else {
                xs[i + 1] - xs[i]
            };
            let key = [
                xs[i].to_bits(),
                gap_l.to_bits(),
                gap_r.to_bits(),
                seam_gap.to_bits(),
            ];
            let (l, r) = match diffs.get(&key) {
                Some(d) => *d,
                None => {
                    let d = (p.derive)(
                        &stations,
                        &sk,
                        i,
                        DERIV_STEP * gap_l,
                        DERIV_STEP * gap_r,
                        DERIV_STEP * seam_gap,
                        true,
                    )?;
                    diffs.insert(key, d);
                    d
                }
            };
            lefts.push(l);
            rights.push(r);
        }
        let surface = (p.skin_of)(&sk, &lefts, &rights, &xs)?;
        let mut mids: Vec<Station> = Vec::with_capacity(m - 1);
        let mut bad: Vec<usize> = Vec::new();
        for i in 0..m - 1 {
            let mut key = vec![xs[i].to_bits(), xs[i + 1].to_bits()];
            tangent_bits(&rights[i], &mut key);
            tangent_bits(&lefts[i + 1], &mut key);
            let result = match spans.get(&key) {
                Some(r) => *r,
                None => {
                    let r = (p.check_span)(&surface, &stations[i..i + 2], &stations[i])?;
                    spans.insert(key, r);
                    r
                }
            };
            match result {
                Some(mid) => mids.push(mid),
                None => bad.push(i),
            }
        }
        if bad.is_empty() {
            return Ok((surface, stations, mids));
        }
        let too_short = bad
            .iter()
            .any(|&i| xs[i + 1] - xs[i] <= 2.0 * MIN_SPAN_FRACTION * length);
        if too_short || m + bad.len() > MAX_FREEFORM_STATIONS {
            return Err(BlendError::Unsupported(inaccurate()));
        }
        let halves: Vec<f64> = bad.iter().map(|&i| 0.5 * (xs[i] + xs[i + 1])).collect();
        xs.extend(halves);
        xs.sort_by(f64::total_cmp);
    }
}

/// Baut die Kantengeometrie aus der geprüften Hautfläche: Außenrichtung (an jeder Station und
/// Mitte gleich, sonst klappt die Fläche um), Berührkurven, Volumen nach Simpson.
#[allow(clippy::too_many_arguments)]
fn finish(
    edge: Id<Edge>,
    sides: &EdgeSides,
    convex: bool,
    closed: bool,
    length: f64,
    surface: NurbsSurface,
    stations: &[Station],
    mids: &[Station],
) -> Result<EdgeGeom, BlendError> {
    let contact_a = Curve::nurbs(surface.v_boundary(false).map_err(invalid)?);
    let contact_b = Curve::nurbs(surface.v_boundary(true).map_err(invalid)?);
    let blend_surface = Surface::nurbs(surface, closed, false).map_err(|err| {
        BlendError::Unsupported(tr!("kernel-blend-surface-invalid", err = err.to_string()))
    })?;
    let sense_at = |st: &Station| {
        let natural = blend_surface.normal(st.x, 0.5);
        let p = blend_surface.point(st.x, 0.5);
        (natural != DVec3::ZERO && natural.is_finite())
            .then(|| outward_sense(natural, p, st.o, convex))
    };
    let first = &stations[0];
    let same_sense = sense_at(first).ok_or(BlendError::SpineLost)?;
    if stations
        .iter()
        .chain(mids)
        .any(|st| sense_at(st) != Some(same_sense))
    {
        return Err(BlendError::Unsupported(folded()));
    }
    // Volumen = ∫ A(s) · (c′(s) · T(s)) ds (Querschnitte in den Ebenen senkrecht zur Kante,
    // c = Schwerpunkt): je Stationsabstand Simpson, c′ aus der Parabel durch die drei Schwerpunkte.
    // Bei geraden Kanten ist c′ · T = 1, also genau ∫ A ds.
    let mut volume = 0.0;
    for (w, mid) in stations.windows(2).zip(mids) {
        let h = w[1].x - w[0].x;
        let ((a0, c0), (am, cm), (a1, c1)) =
            (section_area(&w[0]), section_area(mid), section_area(&w[1]));
        let d0 = (cm * 4.0 - c0 * 3.0 - c1) / h;
        let dm = (c1 - c0) / h;
        let d1 = (c0 - cm * 4.0 + c1 * 3.0) / h;
        volume += h / 6.0
            * (a0 * d0.dot(w[0].tangent)
                + 4.0 * am * dm.dot(mid.tangent)
                + a1 * d1.dot(w[1].tangent));
    }
    if !(volume.is_finite() && volume > 0.0) {
        return Err(BlendError::ResultInvalid(tr!(
            "kernel-blend-volume-estimate-invalid"
        )));
    }
    let reach = stations
        .iter()
        .chain(mids)
        .map(|st| st.sec.pa.distance(st.o).max(st.sec.pb.distance(st.o)))
        .fold(0.0, f64::max);
    let last = &stations[stations.len() - 1];
    Ok(EdgeGeom {
        edge,
        sides: *sides,
        convex,
        same_sense,
        surface: blend_surface,
        contact_a,
        contact_b,
        range: (0.0, length),
        section_start: section_seg(first.kind, &first.sec),
        section_end: section_seg(last.kind, &last.sec),
        volume: VolumeModel::Total { volume },
        reach,
        closed,
        analytic: false,
    })
}
