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
//! `NURBS_EDGE_TOL / 2` treffen; sonst benannte Ablehnung ([`INACCURATE`]).
//!
//! Freiformflächen und -kanten (Task 15) laufen denselben Weg; zusätzlich wird der Startwert in
//! die Querschnittsebene gelegt, jeder Berührpunkt auf einer Freiformfläche als echter Fußpunkt
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
/// Meldung bei schrägem Abschluss einer Hautfläche (P5).
pub const OBLIQUE_END: &str = "Eine Freiform- oder variable Rundung lässt sich nur an einer Ebene senkrecht zur Kante abschließen";
/// Meldung, wenn die Hautfläche die geforderte Genauigkeit nicht erreicht.
pub const INACCURATE: &str =
    "Die Rundungsfläche lässt sich nicht genau genug annähern; bitte Maß oder Form leicht ändern";
/// Meldung, wenn Querschnitte stillstehen oder die Fläche umklappt.
pub const FOLDED: &str =
    "Die Rundungsfläche würde sich selbst überschneiden; bitte ein kleineres Maß wählen";
/// Meldung für Ecken mit variablem Radius (P6, Task 17).
pub const VARIABLE_CORNER: &str =
    "Ecken mit variablem Radius oder Freiform-Rundungsflächen werden noch nicht unterstützt";

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
            .map_err(|e| {
                BlendError::Unsupported(format!("Ein Querschnitt der Rundung ist ungültig: {e}"))
            }),
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
fn plane_start(a: Side, b: Side, o: DVec3, radius: f64, convex: bool) -> Option<DVec3> {
    let (na, nb) = (a.normal_at(o), b.normal_at(o));
    let denom = 1.0 + na.dot(nb);
    let s = if convex { -1.0 } else { 1.0 };
    let c = o + (na + nb) * (s * radius / denom);
    (denom > MIN_BISECTOR_START && c.is_finite()).then_some(c)
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
    BlendError::Unsupported(format!("Die Rundungsfläche ist ungültig: {err}"))
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
fn check_spine(e: &Edge, t: f64, st: &Station) -> Result<(), BlendError> {
    let near_axis = || {
        BlendError::Unsupported(
            "Die Kugelbahn kommt der Drehachse näher als der Radius; bitte einen kleineren Radius wählen"
                .into(),
        )
    };
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
            let d1 = e.curve.tangent(t);
            let d2 = (e.curve.tangent(tb) - e.curve.tangent(ta)) / (tb - ta);
            let speed2 = d1.length_squared();
            let kvec = (d2 - d1 * (d2.dot(d1) / speed2)) / speed2;
            let reach = match st.kind {
                SectionKind::Fillet { .. } => {
                    (st.sec.center - st.o).dot(kvec) + reach_r * arc_extent(&st.sec, kvec)
                }
                SectionKind::Chamfer(_) => (st.sec.pa - st.o)
                    .dot(kvec)
                    .max((st.sec.pb - st.o).dot(kvec)),
            };
            if !(kvec.is_finite() && reach < EDGE_CURVATURE_LIMIT) {
                return Err(BlendError::Unsupported(
                    "Die Rundung ist für die Krümmung der Kante zu groß; bitte ein kleineres Maß wählen"
                        .into(),
                ));
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
        return Err(BlendError::InvalidParameter(format!(
            "Die Berührpunkte liegen nicht weiter als {} mm auseinander; das Maß ist zu klein",
            super::show_num(min)
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
        return Err(BlendError::InvalidParameter(
            "Auf einer geschlossenen Kante muss der Radius am Anfang und am Ende gleich sein"
                .into(),
        ));
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
        let tangent = (e.curve.tangent(t) * (e.t1 - e.t0).signum()).normalize_or_zero();
        let kind = match law {
            SectionLaw::Fillet(r) => SectionKind::Fillet { radius: r },
            SectionLaw::Variable(l) => SectionKind::Fillet {
                radius: radius_at(l, x),
            },
            SectionLaw::Chamfer(spec) => SectionKind::Chamfer(spec),
        };
        let sec = match kind {
            SectionKind::Fillet { radius } => {
                check_curvature(a, o, convex, radius, &name_a)?;
                check_curvature(b, o, convex, radius, &name_b)?;
                if freeform {
                    // Krümmung der Freiformflächen schon am Kantenpunkt (zur Kugel hin).
                    let ball = if convex { -1.0 } else { 1.0 };
                    check_freeform_curvature(a, o, a.normal_at(o) * ball, radius, &name_a)?;
                    check_freeform_curvature(b, o, b.normal_at(o) * ball, radius, &name_b)?;
                }
                // Ohne Nachbarstation: bei Freiformbeteiligung die Ebenenformel als Startwert,
                // den `fillet_section` in die Querschnittsebene legt (die numerischen Normalen
                // stehen nicht genau senkrecht zur Kante; Task-2-Hinweis).
                let start = match prev {
                    Some(p) => Some(shifted_start(a, b, o, p, radius, convex)),
                    None if freeform => plane_start(a, b, o, radius, convex),
                    None => None,
                };
                let sec = match fillet_section(a, b, o, tangent, convex, radius, start) {
                    // Ein unpassender Startwert (sehr steiles Gesetz) darf die Bahn nicht
                    // abbrechen lassen: noch einmal mit der Ebenenformel am Kantenpunkt.
                    Err(BlendError::SpineLost) if start.is_some() => {
                        fillet_section(a, b, o, tangent, convex, radius, None)?
                    }
                    other => other?,
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
                chamfer_section(a, b, o, tangent, convex, wa, wb, spec)?
            }
        };
        let st = Station {
            x,
            o,
            tangent,
            sec,
            kind,
        };
        check_section(&st, tol)?;
        check_spine(e, t, &st)?;
        budget()?;
        Ok(st)
    };
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
        // Ableitungen je Station, links (für das Stück davor) und rechts (danach). Glatte innere
        // Stationen: zentrale Differenz. Kantenenden und Knicke des Gesetzes: einseitige
        // Differenzen zweiter Ordnung je Seite (am Knick verschieden, die Fläche ist dort nur
        // stetig). Naht einer geschlossenen Kante: zentral über die Naht, außer beim variablen
        // Radius (Steigung am Kettenanfang und -ende darf verschieden sein, dann je Seite
        // einseitig). Der Schritt bleibt unter `MAX_STEP_GAP` des Abstands zur Nachbarstation, so
        // überspannt keine Differenz einen Knick (Knicke sind Stationen).
        let seam_central = closed && !matches!(law, SectionLaw::Variable(_));
        let mut lefts = Vec::with_capacity(m);
        let mut rights = Vec::with_capacity(m);
        for (i, st) in stations.iter().enumerate() {
            let at =
                |x: f64| -> Result<SkinSection, BlendError> { to_skin(&section_at(x, Some(st))?) };
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
                let s0 = sk[i];
                let (s1, s2) = (at(st.x + dir * step)?, at(st.x + 2.0 * dir * step)?);
                Ok((one_sided(&s0, &s1, &s2, dir * step), s0, s1, step))
            };
            let pair = if (first || last) && seam_central {
                let (sl, sh) = (at(length - delta)?, at(delta)?);
                let t = (
                    SkinTangent::between(&sl, &sh, 2.0 * delta),
                    sl,
                    sh,
                    2.0 * delta,
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
                    return Err(BlendError::Unsupported(FOLDED.into()));
                }
            }
            lefts.push(pair.0.0);
            rights.push(pair.1.0);
        }
        let surface = skin_sided(&sk, &lefts, &rights, &xs).map_err(|err| {
            BlendError::Unsupported(format!("Die Rundungsfläche lässt sich nicht bilden: {err}"))
        })?;
        // Nachprüfung gegen exakte Querschnitte; die Mitten dienen auch der Volumenschätzung.
        let mut mids: Vec<Station> = Vec::with_capacity(m - 1);
        let mut accurate = true;
        'check: for (i, w) in stations.windows(2).enumerate() {
            for f in CHECK_U {
                let xu = w[0].x + (w[1].x - w[0].x) * f;
                let exact = section_at(xu, Some(&stations[i]))?;
                let want = to_skin(&exact)?;
                let off = CHECK_V
                    .iter()
                    .any(|&v| !(surface.point(xu, v).distance(want.point(v)) <= SKIN_TOL));
                let loose = !(a.outside(surface.point(xu, 0.0)).abs() <= CONTACT_TOL
                    && b.outside(surface.point(xu, 1.0)).abs() <= CONTACT_TOL);
                budget()?;
                if off || loose {
                    accurate = false;
                    break 'check;
                }
                if f == 0.5 {
                    mids.push(exact);
                }
            }
        }
        if accurate {
            return finish(
                edge, sides, convex, closed, length, surface, &stations, &mids,
            );
        }
        if n >= MAX_STATIONS {
            return Err(BlendError::Unsupported(INACCURATE.into()));
        }
        n = 2 * n - 1;
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
        BlendError::Unsupported(format!("Die Rundungsfläche ist ungültig: {err}"))
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
        return Err(BlendError::Unsupported(FOLDED.into()));
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
        return Err(BlendError::ResultInvalid(
            "Die Volumenschätzung der Rundung ist ungültig".into(),
        ));
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
