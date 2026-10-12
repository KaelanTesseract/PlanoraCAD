//! Numerischer Schnittrechner für Flächenpaare, von denen mindestens eine eine Freiformfläche ist
//! (Spec §6, Entscheidungen N3, N4, N7).
//!
//! Alle Rechnungen laufen über die vorzeichenbehafteten Abstandsfunktionen `F_a`, `F_b` der beiden
//! Flächen; der Gradient ist die Einheitsnormale am Fußpunkt (für analytische und Freiformflächen
//! gleich). Ablauf:
//!
//! 1. **Zusammenfallend?** Stichproben der Freiformfläche(n) auf der anderen Fläche.
//! 2. **Startpunkte:** Zerlegung in Bézier-Flicken, rekursive Unterteilung mit sicherem Ausschluss
//!    (Hüllquader bzw. Lipschitz-Schranke des Abstands), Newton im Raum auf beide Flächen.
//! 3. **Verfolgen** in beide Richtungen (Vorhersage längs `g_a × g_b`, Korrektur mit 3-D-Newton),
//!    Schrittweite nach Krümmung und Sehnenfehler; Ende bei Rückkehr zum Anfang, am Rand der
//!    Freiformfläche (Bisektion), beim Treffen auf eine andere Kurve (gemeldet) oder an den Grenzen.
//! 4. **Ordnen und Annähern:** Startpunkte auf schon verfolgten Kurven entfallen, Zweige mit
//!    gemeinsamen Enden werden verbunden, jede Kurve wird mit [`NurbsCurve::fit`] angenähert und
//!    zwischen den Punkten gegen beide Flächen geprüft.
//!
//! Nicht lösbare Lagen (Verzweigung, tangentiale Berührung neben echten Schnitten, teilweise
//! zusammenfallende Flächen, überschrittene Grenzen, Zahlenfehler) ergeben
//! [`SsiOutcome::Unsupported`] mit deutscher Meldung, nie ein stilles Teilergebnis.

use std::collections::HashMap;

use crate::geometry::{Curve, Surface};
use crate::nurbs_surface::NurbsSurfaceData;
use planora_math::nurbs::{BezierPatch, NurbsCurve};
use planora_math::{DVec3, TAU, Tolerance};

/// Fehlerschranke der Annäherung einer Schnittkurve (N4/Q5) in mm.
pub const NURBS_FIT_TOL: f64 = 1e-5;
/// Größte Unterteilungstiefe der Flicken bei der Startpunktsuche.
pub const MAX_SUBDIVISION_DEPTH: usize = 12;
/// Höchstzahl Startpunkte je Flächenpaar.
pub const MAX_SEEDS: usize = 512;
/// Höchstzahl Schritte je Verfolgung (beide Richtungen zusammen).
pub const MAX_MARCH_STEPS: usize = 20_000;
/// Größte Kurvenlänge = Faktor · Diagonale der Hüllquader beider Flächen.
pub const MAX_CURVE_LENGTH_FACTOR: f64 = 20.0;

/// Ein Flicken gilt als eben genug, wenn sein Ebenheitsmaß höchstens dieser Anteil seiner
/// Diagonale ist.
const SEED_FLAT_FACTOR: f64 = 0.05;
/// Zusätzlich muss ein Kandidat-Flicken klein sein: Diagonale höchstens dieser Anteil der
/// Gesamtdiagonale (sonst fände ein großer ebener Flicken mit zwei Schnittkurven nur eine).
const SEED_PATCH_FRACTION: f64 = 1.0 / 16.0;
/// Gegen eine gekrümmte analytische Fläche zusätzlich höchstens dieses Vielfache ihres kleinsten
/// Krümmungsradius (Zylinder-, Kegel-, Kugelradius, Rohrradius des Torus): Zwei Schnittkurven,
/// die enger beieinander liegen, teilten sich sonst leicht einen Kandidaten.
const SEED_FEATURE_FACTOR: f64 = 1.0;
/// Absolute Untergrenze der Flickendiagonale (mm), unter der nicht mehr unterteilt wird.
const MIN_PATCH_DIAG: f64 = 1e-4;
/// Höchstzahl geprüfter Flicken bzw. Flickenpaare bei der Unterteilung (Arbeitsgrenze).
const MAX_PATCH_VISITS: usize = 200_000;
/// Höchstzahl Kandidaten-Flicken (jeder liefert höchstens einen Startpunkt).
const MAX_CANDIDATES: usize = 8 * MAX_SEEDS;
/// Liegt der gefundene Startpunkt nicht beim Kandidaten (Hüllquader, vergrößert um diesen Anteil
/// der Flickendiagonale), wird der Kandidat bis zu `SEED_RETRY_LEVELS`-mal weiter unterteilt.
const SEED_LOCAL_FACTOR: f64 = 1.0;
const SEED_RETRY_LEVELS: usize = 2;
/// Höchstzahl Newton-Schritte bei der Suche eines Startpunkts.
const SEED_NEWTON_STEPS: usize = 50;
/// Größter Newton-Schritt bei der Startpunktsuche als Anteil der Gesamtdiagonale.
const SEED_MAX_MOVE: f64 = 0.25;
/// Unterhalb dieser Gram-Determinante `1 − (g_a·g_b)²` (= `|g_a × g_b|²`) wird abwechselnd auf
/// beide Flächen projiziert statt der gemeinsamen Newton-Schritt gerechnet.
const SEED_DET_MIN: f64 = 1e-20;
/// Konvergenz: `|F_a|, |F_b| ≤ CONVERGE_REL · max(Diagonale, 1 mm)`, mindestens
/// `COORD_ULPS · ε · größter Koordinatenbetrag` (Auflösung von f64 fern vom Ursprung).
const CONVERGE_REL: f64 = 1e-9;
const COORD_ULPS: f64 = 64.0;
/// Startpunkte (und Punkte gleicher Kurve) näher als dies (mm) gelten als derselbe Punkt.
const SAME_POINT: f64 = 1e-6;
/// `|g_a × g_b|` darunter: Normalen parallel (tangentiale Berührung).
const PARALLEL_NORMALS: f64 = 1e-4;
/// Größte Schrittweite = Diagonale / `STEP_DIVISOR`.
const STEP_DIVISOR: f64 = 200.0;
/// Kleinste Schrittweite (mm).
const H_MIN: f64 = 1e-4;
/// Rückkehr zum Anfang frühestens nach so vielen Punkten.
const RETURN_STEPS: usize = 8;
/// Rückkehr zum Anfang: Abstand unter `RETURN_FACTOR · h`.
const RETURN_FACTOR: f64 = 1.5;
/// Treffen auf eine andere verfolgte Kurve: Abstand unter `HIT_FACTOR · h`.
const HIT_FACTOR: f64 = 0.5;
/// Ein Startpunkt wird nur dann gegen eine verfolgte Kurve geprüft, wenn sie näher als
/// `SKIP_FACTOR · h_max` liegt.
const SKIP_FACTOR: f64 = 2.0;
/// Nachbarstücke derselben Verfolgung innerhalb dieser Bogenlänge (· h_max) zählen nicht als
/// Treffer.
const NEIGHBOR_ARC: f64 = 3.0;
/// Zellgröße des Suchrasters für Polylinienstücke (· h_max; muss die Stücklänge abdecken).
const CELL_FACTOR: f64 = 4.0;
/// Enden zweier Zweige näher als dies (mm) werden verbunden.
const JOIN_TOL: f64 = 1e-6;
/// Höchstzahl Newton-Schritte der Korrektur.
const CORRECTOR_STEPS: usize = 20;
/// Die Korrektur gilt als gescheitert, wenn ein Schritt weiter als dieses Vielfache der
/// Schrittweite führt.
const CORRECT_REACH: f64 = 2.0;
/// Größte Drehung der Tangente je Schritt (Bogenmaß).
const MAX_TURN: f64 = 0.35;
/// Ein Schritt darf höchstens so viel weiter als die Schrittweite führen.
const STEP_REACH: f64 = 1.5;
/// Sehnenfehler je Schritt höchstens dieses Vielfache von `NURBS_FIT_TOL`.
const SAGITTA_FACTOR: f64 = 2.0;
/// Größtes Wachstum der Schrittweite je Schritt.
const MAX_GROWTH: f64 = 2.0;
/// Genauigkeit des Randpunkts bei der Bisektion (mm).
const BISECT_TOL: f64 = 1e-9;
/// Höchstzahl Bisektionsschritte.
const BISECT_STEPS: usize = 60;
/// Stichproben je Richtung für die Prüfung auf zusammenfallende Flächen (10 · 10 = 100).
const COINCIDENT_SAMPLES: usize = 10;
/// Anteil der Stichproben, die auf der anderen Fläche liegen müssen.
const COINCIDENT_SHARE: f64 = 0.9;
/// Liegt mindestens dieser Anteil (aber weniger als `COINCIDENT_SHARE`) auf der anderen Fläche,
/// fallen die Flächen teilweise zusammen. Eine Berührlinie trifft höchstens eine Zeile, Spalte
/// oder Diagonale des 10 · 10-Rasters (unter 20 %), eine flächige Überdeckung ab einem Viertel.
const PARTIAL_SHARE: f64 = 0.25;
/// Abstand der Proben um einen Berührpunkt (Anteil der Diagonale) und ihre Zahl.
const CONTACT_PROBE: f64 = 0.01;
const CONTACT_DIRECTIONS: usize = 8;
/// Höchstzahl Berührpunkte, die einzeln untersucht werden.
const MAX_TOUCH_CHECKS: usize = 64;
/// Annäherungsversuche mit jeweils um diesen Faktor kleinerer Schranke.
const FIT_ATTEMPTS: usize = 3;
const FIT_SHRINK: f64 = 0.25;

const MSG_NUMERIC: &str = "Numerischer Fehler beim Flächenschnitt (ungültiger Zahlenwert)";
const MSG_TANGENTIAL: &str =
    "Die Flächen berühren sich tangential; der Schnitt lässt sich dort nicht sicher bestimmen";
const MSG_TOUCH_AND_CUT: &str =
    "Die Flächen berühren sich tangential an einer Stelle und schneiden sich an einer anderen";
const MSG_TOUCH_CROSSING: &str = "Die Flächen berühren sich tangential und durchdringen sich dabei";
const MSG_BRANCH: &str = "Die Schnittkurve verzweigt sich oder trifft eine andere Schnittkurve";
const MSG_LOST: &str =
    "Die Schnittkurve lässt sich nicht verfolgen (Schrittweite unter der Grenze)";
const MSG_FIT: &str = "Die Schnittkurve lässt sich nicht annähern";
const MSG_PARTIAL: &str = "Die Flächen fallen teilweise zusammen";

fn msg_too_many() -> String {
    format!("Zu viele Schnittbereiche (mehr als {MAX_SEEDS} Startpunkte oder zu viele Flicken)")
}

fn msg_too_long() -> String {
    format!(
        "Die Schnittkurve ist zu lang (mehr als {MAX_MARCH_STEPS} Schritte oder länger als das \
         {MAX_CURVE_LENGTH_FACTOR}-Fache der Flächengröße)"
    )
}

/// Ergebnis des numerischen Schnitts.
#[derive(Clone, Debug)]
pub enum SsiOutcome {
    /// Schnittkurven (je eine angenäherte Freiformkurve, geschlossene mit Anfang = Ende).
    Curves(Vec<Curve>),
    /// Kein gemeinsamer Punkt.
    None,
    /// Nur Berührung ohne Durchdringung.
    Touch,
    /// Die Flächen fallen zusammen.
    Coincident,
    /// Nicht lösbare Lage; deutsche Meldung.
    Unsupported(String),
}

/// Schnitt zweier Flächen, von denen mindestens eine `Surface::Nurbs` ist.
pub fn intersect_nurbs(a: &Surface, b: &Surface, tol: &Tolerance) -> SsiOutcome {
    if !matches!(a, Surface::Nurbs(_)) && !matches!(b, Surface::Nurbs(_)) {
        return SsiOutcome::Unsupported(
            "Der numerische Schnittrechner braucht mindestens eine Freiformfläche".into(),
        );
    }
    let mut ssi = Ssi::new(a, b, *tol);
    ssi.run().unwrap_or_else(SsiOutcome::Unsupported)
}

type Uv = (f64, f64);

/// Auswertung einer Abstandsfunktion: Wert, Gradient (Einheitsnormale am Fußpunkt), Fußpunkt
/// (nur Freiformflächen) und ob der Fußpunkt auf einem offenen Rand liegt.
#[derive(Clone, Copy, Debug)]
struct Eval {
    f: f64,
    g: DVec3,
    foot: Option<Uv>,
    edge: bool,
}

/// Startwerte der Fußpunktsuche auf beiden Seiten.
#[derive(Clone, Copy, Debug, Default)]
struct Hints {
    a: Option<Uv>,
    b: Option<Uv>,
}

/// Punkt mit beiden Auswertungen.
#[derive(Clone, Copy, Debug)]
struct Probe {
    q: DVec3,
    ea: Eval,
    eb: Eval,
}

impl Probe {
    fn hints(&self) -> Hints {
        Hints {
            a: self.ea.foot,
            b: self.eb.foot,
        }
    }

    fn cross(&self) -> DVec3 {
        self.ea.g.cross(self.eb.g)
    }

    fn residual(&self) -> f64 {
        self.ea.f.abs().max(self.eb.f.abs())
    }

    fn on_edge(&self) -> bool {
        self.ea.edge || self.eb.edge
    }
}

enum Fail {
    /// Die Fußpunktsuche fand nichts (oder die Normale fehlt).
    NoFoot,
    /// NaN oder unendlich.
    Numeric,
}

/// Eine Seite des Schnitts.
struct Side<'a> {
    surf: &'a Surface,
    nurbs: Option<&'a NurbsSurfaceData>,
}

impl<'a> Side<'a> {
    fn new(surf: &'a Surface) -> Self {
        let nurbs = match surf {
            Surface::Nurbs(d) => Some(&**d),
            _ => None,
        };
        Side { surf, nurbs }
    }

    fn eval(&self, q: DVec3, hint: Option<Uv>) -> Result<Eval, Fail> {
        if !q.is_finite() {
            return Err(Fail::Numeric);
        }
        let Some(d) = self.nurbs else {
            let f = self.surf.implicit(q);
            let (u, v) = self.surf.project(q);
            let g = self.surf.normal(u, v);
            if !f.is_finite() || !g.is_finite() {
                return Err(Fail::Numeric);
            }
            return Ok(Eval {
                f,
                g,
                foot: None,
                edge: false,
            });
        };
        let (u, v, dist) = d
            .closest(q, hint)
            .or_else(|| d.closest(q, None))
            .ok_or(Fail::NoFoot)?;
        let s = d.point(u, v);
        let n = d.normal(u, v);
        if !s.is_finite() || !n.is_finite() || !dist.is_finite() {
            return Err(Fail::Numeric);
        }
        if n == DVec3::ZERO {
            return Err(Fail::NoFoot);
        }
        // Voller Abstand mit Vorzeichen: jenseits des Rands bleibt er positiv (nie falsche Null).
        let f = if (q - s).dot(n) < 0.0 { -dist } else { dist };
        Ok(Eval {
            f,
            g: n,
            foot: Some((u, v)),
            edge: on_open_edge(d, u, v),
        })
    }

    /// Fußpunkt (Punkt auf der Fläche) einer Auswertung an `q`.
    fn foot_point(&self, q: DVec3, e: &Eval) -> DVec3 {
        match (self.nurbs, e.foot) {
            (Some(d), Some((u, v))) => d.point(u, v),
            _ => q - e.g * e.f,
        }
    }
}

/// Liegt `(u, v)` auf dem Rand einer offenen Richtung?
fn on_open_edge(d: &NurbsSurfaceData, u: f64, v: f64) -> bool {
    let ((u0, u1), (v0, v1)) = d.surface().domain();
    (!d.is_periodic_u() && (u <= u0 || u >= u1)) || (!d.is_periodic_v() && (v <= v0 || v >= v1))
}

/// Knoten der Unterteilung: je Freiformseite ein Flicken.
#[derive(Clone)]
struct Node {
    pa: Option<BezierPatch>,
    pb: Option<BezierPatch>,
    depth: usize,
    retries: usize,
}

/// Ergebnis der Korrektur.
enum Corr {
    Ok(Probe),
    /// Gescheitert, Fußpunkt am offenen Rand: der Punkt liegt jenseits der Freiformfläche.
    Outside,
    /// Normalen (fast) parallel.
    Singular,
    Failed,
}

/// Ergebnis eines Schritts.
struct Step {
    probe: Probe,
    t: DVec3,
    dist: f64,
    turn: f64,
    boundary: bool,
}

/// Verfolgte Halbkurve (Polylinie) samt Bogenlänge und Startwerten je Punkt.
struct Poly {
    pts: Vec<DVec3>,
    arc: Vec<f64>,
    hints: Vec<Hints>,
    trace: usize,
}

/// Fertiger Zweig.
struct Branch {
    pts: Vec<DVec3>,
    hints: Vec<Hints>,
    closed: bool,
}

#[derive(Default)]
struct Budget {
    steps: usize,
    length: f64,
}

type CellKey = (i64, i64, i64);

struct Ssi<'a> {
    a: Side<'a>,
    b: Side<'a>,
    tol: Tolerance,
    diag: f64,
    eps: f64,
    /// Auflösung der Koordinaten (siehe `COORD_ULPS`).
    res: f64,
    h_max: f64,
    polys: Vec<Poly>,
    grid: HashMap<CellKey, Vec<(usize, usize)>>,
    cell: f64,
    visits: usize,
    /// Größte Diagonale eines Kandidaten-Flickens.
    max_patch: f64,
}

/// Kleinster Krümmungsradius einer analytischen Fläche (Ebene, Freiformfläche: keiner).
fn feature_size(s: &Surface) -> f64 {
    let r = match s {
        Surface::Cylinder { radius, .. }
        | Surface::Cone { radius, .. }
        | Surface::Sphere { radius, .. } => radius.abs(),
        Surface::Torus { minor, .. } => minor.abs(),
        Surface::Plane { .. } | Surface::Nurbs(_) => f64::INFINITY,
    };
    if r.is_finite() && r > MIN_PATCH_DIAG {
        r
    } else {
        f64::INFINITY
    }
}

impl<'a> Ssi<'a> {
    fn new(a: &'a Surface, b: &'a Surface, tol: Tolerance) -> Self {
        let (sa, sb) = (Side::new(a), Side::new(b));
        let (lo, hi) = [sa.nurbs, sb.nurbs].iter().flatten().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(lo, hi), d| {
                let (l, h) = d.control_bounds();
                (lo.min(l), hi.max(h))
            },
        );
        let diag = (hi - lo).length();
        let h_max = (diag / STEP_DIVISOR).max(H_MIN);
        let res = COORD_ULPS * f64::EPSILON * lo.abs().max(hi.abs()).max_element();
        Ssi {
            a: sa,
            b: sb,
            tol,
            diag,
            // Bei großen Koordinaten begrenzt die Auflösung von f64 die erreichbare Genauigkeit.
            eps: (CONVERGE_REL * diag.max(1.0)).max(res),
            res,
            h_max,
            polys: Vec::new(),
            grid: HashMap::new(),
            cell: CELL_FACTOR * h_max,
            visits: 0,
            max_patch: (SEED_PATCH_FRACTION * diag)
                .min(SEED_FEATURE_FACTOR * feature_size(a))
                .min(SEED_FEATURE_FACTOR * feature_size(b)),
        }
    }

    fn run(&mut self) -> Result<SsiOutcome, String> {
        let lin = self.tol.linear;
        if !self.diag.is_finite() || !self.eps.is_finite() || !(lin.is_finite() && lin > 0.0) {
            return Err(MSG_NUMERIC.into());
        }
        let shares: Vec<f64> = [
            self.share_on(&self.a, &self.b),
            self.share_on(&self.b, &self.a),
        ]
        .into_iter()
        .flatten()
        .collect();
        if shares.iter().all(|s| *s >= COINCIDENT_SHARE) {
            return Ok(SsiOutcome::Coincident);
        }
        if shares.iter().any(|s| *s >= PARTIAL_SHARE) {
            return Err(MSG_PARTIAL.into());
        }
        // Startpunkte und Verfolgung verschränkt: Ein Startpunkt auf einer schon verfolgten
        // Kurve zählt nicht als eigener Startpunkt.
        let mut stack = self.candidates()?;
        stack.reverse();
        let mut starts: Vec<DVec3> = Vec::new();
        let mut touches: Vec<Probe> = Vec::new();
        let mut branches = Vec::new();
        while let Some(n) = stack.pop() {
            let Some(s) = self.seed_of(&n, &mut stack)? else {
                continue;
            };
            if starts.iter().any(|q| q.distance(s.q) <= SAME_POINT) {
                continue;
            }
            let transversal = s.cross().length() >= PARALLEL_NORMALS;
            if transversal && self.on_traced(&s)? {
                continue;
            }
            starts.push(s.q);
            if starts.len() > MAX_SEEDS {
                return Err(msg_too_many());
            }
            if !transversal {
                touches.push(s);
                continue;
            }
            let br = self.trace(&s, starts.len())?;
            let len: f64 = br.pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            if len <= self.tol.linear {
                // Nur ein Punkt (etwa an einer Ecke der Freiformfläche): wie eine Berührung.
                touches.push(s);
            } else {
                branches.push(br);
            }
        }
        let branches = join(branches);
        if !branches.is_empty() {
            if !touches.is_empty() {
                return Err(MSG_TOUCH_AND_CUT.into());
            }
            let curves = branches
                .iter()
                .map(|b| self.fit_branch(b))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(SsiOutcome::Curves(curves));
        }
        if !touches.is_empty() {
            return self.classify_touch(&touches);
        }
        Ok(SsiOutcome::None)
    }

    fn probe(&self, q: DVec3, h: Hints) -> Result<Probe, Fail> {
        Ok(Probe {
            q,
            ea: self.a.eval(q, h.a)?,
            eb: self.b.eval(q, h.b)?,
        })
    }

    fn tangent(&self, p: &Probe) -> Option<DVec3> {
        let c = p.cross();
        let l = c.length();
        (l >= PARALLEL_NORMALS && l.is_finite()).then(|| c / l)
    }

    // ---- 1. Zusammenfallende Flächen ----

    /// Anteil der Stichproben (10 · 10 im Parameterbereich) der Freiformseite `s`, die innerhalb
    /// `tol.linear` auf `o` liegen. Analytische Seiten sind unbeschränkt und werden nicht
    /// abgetastet (`None`; „gegenseitig“ prüft dann nur die Freiformseite).
    fn share_on(&self, s: &Side, o: &Side) -> Option<f64> {
        let d = s.nurbs?;
        let ((u0, u1), (v0, v1)) = d.surface().domain();
        let n = COINCIDENT_SAMPLES;
        let mut on = 0usize;
        for i in 0..n {
            for j in 0..n {
                let u = u0 + (u1 - u0) * (i as f64 + 0.5) / n as f64;
                let v = v0 + (v1 - v0) * (j as f64 + 0.5) / n as f64;
                let f = o.surf.implicit(d.point(u, v));
                if f.is_finite() && f.abs() <= self.tol.linear {
                    on += 1;
                }
            }
        }
        Some(on as f64 / (n * n) as f64)
    }

    // ---- 2. Startpunkte ----

    fn count_visit(&mut self) -> Result<(), String> {
        self.visits += 1;
        if self.visits > MAX_PATCH_VISITS {
            return Err(msg_too_many());
        }
        Ok(())
    }

    /// Kann der Flicken die analytische Fläche `other` nicht treffen? Sicher, weil die
    /// Abstandsfunktionen analytischer Flächen 1-Lipschitz sind: Liegt der Mittelpunkt des
    /// Hüllquaders weiter als dessen halbe Diagonale von der Fläche, liegt kein Punkt des Quaders
    /// (und damit des Flickens) auf ihr. Für die Ebene schließt zusätzlich „alle Kontrollpunkte
    /// echt auf einer Seite“ aus.
    fn excluded_by(&self, other: &Side, p: &BezierPatch) -> bool {
        let (lo, hi) = p.bounds();
        let c = (lo + hi) * 0.5;
        let r = 0.5 * (hi - lo).length();
        let f = other.surf.implicit(c);
        if f.is_finite() && f.abs() > r + self.tol.linear {
            return true;
        }
        if let Surface::Plane { .. } = other.surf {
            let fs: Vec<f64> = p.control.iter().map(|q| other.surf.implicit(*q)).collect();
            let lin = self.tol.linear;
            if fs.iter().all(|f| *f > lin) || fs.iter().all(|f| *f < -lin) {
                return true;
            }
        }
        false
    }

    fn keep(&self, n: &Node) -> bool {
        match (&n.pa, &n.pb) {
            (Some(p), None) => !self.excluded_by(&self.b, p),
            (None, Some(p)) => !self.excluded_by(&self.a, p),
            (Some(p), Some(q)) => {
                let (la, ha) = p.bounds();
                let (lb, hb) = q.bounds();
                let m = DVec3::splat(self.tol.linear);
                (la - m).cmple(hb).all() && (lb - m).cmple(ha).all()
            }
            (None, None) => false,
        }
    }

    fn patch_diag(p: &BezierPatch) -> f64 {
        let (lo, hi) = p.bounds();
        (hi - lo).length()
    }

    /// Flicken fertig unterteilt: eben und klein genug, oder zu klein zum Weiterteilen.
    fn patch_done(&self, p: &BezierPatch) -> bool {
        let d = Self::patch_diag(p);
        d < MIN_PATCH_DIAG || (p.flatness() <= SEED_FLAT_FACTOR * d && d <= self.max_patch)
    }

    fn node_done(&self, n: &Node) -> bool {
        n.depth >= MAX_SUBDIVISION_DEPTH
            || [&n.pa, &n.pb]
                .iter()
                .all(|p| p.as_ref().is_none_or(|p| self.patch_done(p)))
    }

    /// Kinder eines Knotens; `force`: auch fertige Flicken teilen (nicht unter `MIN_PATCH_DIAG`).
    fn children(&self, n: &Node, force: bool) -> Vec<Node> {
        let split = |p: &Option<BezierPatch>| -> Vec<Option<BezierPatch>> {
            match p {
                Some(p)
                    if Self::patch_diag(p) >= MIN_PATCH_DIAG && (force || !self.patch_done(p)) =>
                {
                    p.subdivide().into_iter().map(Some).collect()
                }
                other => vec![other.clone()],
            }
        };
        let (ca, cb) = (split(&n.pa), split(&n.pb));
        let mut out = Vec::with_capacity(ca.len() * cb.len());
        for a in &ca {
            for b in &cb {
                out.push(Node {
                    pa: a.clone(),
                    pb: b.clone(),
                    depth: n.depth + 1,
                    retries: n.retries,
                });
            }
        }
        out
    }

    /// Kandidaten-Flicken durch rekursive Unterteilung.
    fn candidates(&mut self) -> Result<Vec<Node>, String> {
        let patches = |s: &Side| -> Result<Option<Vec<BezierPatch>>, String> {
            s.nurbs
                .map(|d| d.surface().bezier_patches())
                .transpose()
                .map_err(|e| format!("Die Freiformfläche lässt sich nicht zerlegen: {e}"))
        };
        let (pa, pb) = (patches(&self.a)?, patches(&self.b)?);
        let mut out = Vec::new();
        let mut stack: Vec<Node> = Vec::new();
        let node = |pa: Option<&BezierPatch>, pb: Option<&BezierPatch>| Node {
            pa: pa.cloned(),
            pb: pb.cloned(),
            depth: 0,
            retries: 0,
        };
        // Wurzeln einzeln einspeisen (bei zwei Freiformflächen die Paare), damit der Stapel klein
        // bleibt.
        let roots: Vec<(Option<usize>, Option<usize>)> = match (&pa, &pb) {
            (Some(x), None) => (0..x.len()).map(|i| (Some(i), None)).collect(),
            (None, Some(y)) => (0..y.len()).map(|j| (None, Some(j))).collect(),
            (Some(x), Some(y)) => {
                if x.len().saturating_mul(y.len()) > MAX_PATCH_VISITS {
                    return Err(msg_too_many());
                }
                (0..x.len())
                    .flat_map(|i| (0..y.len()).map(move |j| (Some(i), Some(j))))
                    .collect()
            }
            (None, None) => Vec::new(),
        };
        for (i, j) in roots {
            let root = node(
                i.and_then(|i| pa.as_ref().map(|v| &v[i])),
                j.and_then(|j| pb.as_ref().map(|v| &v[j])),
            );
            stack.push(root);
            while let Some(n) = stack.pop() {
                self.count_visit()?;
                if !self.keep(&n) {
                    continue;
                }
                if self.node_done(&n) {
                    out.push(n);
                    if out.len() > MAX_CANDIDATES {
                        return Err(msg_too_many());
                    }
                } else {
                    stack.extend(self.children(&n, false));
                }
            }
        }
        Ok(out)
    }

    /// Startpunkt eines Kandidaten (Mittel der Flickenmitten auf den Freiformseiten), Startwerte
    /// und Hüllquader.
    fn cand_start(&self, n: &Node) -> (DVec3, Hints, DVec3, DVec3) {
        let mut pts = Vec::new();
        let mut hints = Hints::default();
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for (patch, side, hint) in [
            (&n.pa, &self.a, &mut hints.a),
            (&n.pb, &self.b, &mut hints.b),
        ] {
            if let (Some(p), Some(d)) = (patch, side.nurbs) {
                let uv = (0.5 * (p.u.0 + p.u.1), 0.5 * (p.v.0 + p.v.1));
                pts.push(d.point(uv.0, uv.1));
                *hint = Some(uv);
                let (l, h) = p.bounds();
                lo = lo.min(l);
                hi = hi.max(h);
            }
        }
        let start = pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64;
        (start, hints, lo, hi)
    }

    /// Newton im Raum auf beide Flächen (kleinster Schritt, der beide linearisierten Gleichungen
    /// erfüllt; bei fast parallelen Normalen abwechselnde Projektion). `None`, wenn nicht
    /// konvergiert.
    fn refine_seed(&self, start: DVec3, hints: Hints) -> Result<Option<Probe>, String> {
        let mut q = start;
        let mut hints = hints;
        let max_move = SEED_MAX_MOVE * self.diag.max(H_MIN);
        for _ in 0..SEED_NEWTON_STEPS {
            let pr = match self.probe(q, hints) {
                Ok(p) => p,
                Err(Fail::NoFoot) => return Ok(None),
                Err(Fail::Numeric) => return Err(MSG_NUMERIC.into()),
            };
            hints = pr.hints();
            let (fa, fb, ga, gb) = (pr.ea.f, pr.eb.f, pr.ea.g, pr.eb.g);
            let c = ga.dot(gb);
            let det = 1.0 - c * c;
            let delta = if det > SEED_DET_MIN {
                let al = (-fa + c * fb) / det;
                let be = (-fb + c * fa) / det;
                ga * al + gb * be
            } else {
                let q1 = q - ga * fa;
                match self.b.eval(q1, hints.b) {
                    Ok(e) => q1 - e.g * e.f - q,
                    Err(Fail::NoFoot) => return Ok(None),
                    Err(Fail::Numeric) => return Err(MSG_NUMERIC.into()),
                }
            };
            if !delta.is_finite() {
                return Err(MSG_NUMERIC.into());
            }
            let len = delta.length();
            if pr.residual() <= self.eps && len <= self.eps {
                return Ok(Some(pr));
            }
            q += if len > max_move {
                delta * (max_move / len)
            } else {
                delta
            };
        }
        match self.probe(q, hints) {
            Ok(pr) => Ok((pr.residual() <= self.eps).then_some(pr)),
            Err(Fail::NoFoot) => Ok(None),
            Err(Fail::Numeric) => Err(MSG_NUMERIC.into()),
        }
    }

    /// Startpunkt eines Kandidaten. Liegt der gefundene Punkt nicht beim Kandidaten (oder findet
    /// sich keiner), kommen dessen Kinder auf den Stapel (höchstens `SEED_RETRY_LEVELS`-mal); ein
    /// gefundener Punkt bleibt trotzdem (er liegt ja auf beiden Flächen).
    fn seed_of(&mut self, n: &Node, stack: &mut Vec<Node>) -> Result<Option<Probe>, String> {
        let (start, hints, lo, hi) = self.cand_start(n);
        let found = self.refine_seed(start, hints)?;
        let m = DVec3::splat(SEED_LOCAL_FACTOR * (hi - lo).length());
        let local = found
            .as_ref()
            .is_some_and(|p| (lo - m).cmple(p.q).all() && p.q.cmple(hi + m).all());
        if !local && n.retries < SEED_RETRY_LEVELS && n.depth < MAX_SUBDIVISION_DEPTH {
            for mut c in self.children(n, true) {
                self.count_visit()?;
                c.retries = n.retries + 1;
                if self.keep(&c) {
                    stack.push(c);
                }
            }
        }
        Ok(found)
    }

    // ---- 3. Verfolgen ----

    /// 3-D-Newton auf `F_a = 0`, `F_b = 0`, `t · (q − plane) = 0` von `start` aus, bis alle drei
    /// Reste höchstens `eps` betragen.
    fn correct(
        &self,
        start: DVec3,
        plane: DVec3,
        t: DVec3,
        hints: Hints,
        reach: f64,
        eps: f64,
    ) -> Result<Corr, String> {
        let mut q = start;
        let mut hints = hints;
        let mut last: Option<Probe> = None;
        let mut prev_res = f64::INFINITY;
        for k in 0..CORRECTOR_STEPS {
            let pr = match self.probe(q, hints) {
                Ok(p) => p,
                Err(Fail::NoFoot) => return Ok(Corr::Failed),
                Err(Fail::Numeric) => return Err(MSG_NUMERIC.into()),
            };
            hints = pr.hints();
            last = Some(pr);
            let r2 = t.dot(q - plane);
            let res = pr.residual();
            if res <= eps && r2.abs() <= eps {
                return Ok(Corr::Ok(pr));
            }
            // Wächst der Rest nach einigen Schritten wieder, divergiert die Korrektur.
            if k >= 4 && res > prev_res {
                break;
            }
            prev_res = res;
            let n = pr.cross();
            if n.length() < PARALLEL_NORMALS {
                return Ok(if pr.on_edge() {
                    Corr::Outside
                } else {
                    Corr::Singular
                });
            }
            let det = t.dot(n);
            if det.abs() < PARALLEL_NORMALS {
                break;
            }
            let (ga, gb) = (pr.ea.g, pr.eb.g);
            let delta = -(gb.cross(t) * pr.ea.f + t.cross(ga) * pr.eb.f + n * r2) / det;
            if !delta.is_finite() {
                return Err(MSG_NUMERIC.into());
            }
            if delta.length() > reach {
                break;
            }
            q += delta;
        }
        Ok(match last {
            Some(p) if p.on_edge() => Corr::Outside,
            _ => Corr::Failed,
        })
    }

    /// Ein Schritt von `p` in Richtung `t` mit Schrittweite `h` (wird bei Bedarf halbiert).
    fn step(&self, p: &Probe, t: DVec3, sign: f64, h: &mut f64) -> Result<Step, String> {
        loop {
            let at_min = *h <= H_MIN;
            let pred = p.q + t * *h;
            let fail = match self.correct(pred, pred, t, p.hints(), CORRECT_REACH * *h, self.eps)? {
                Corr::Ok(q) => match self.tangent(&q) {
                    None => MSG_TANGENTIAL,
                    Some(tq) => {
                        let tq = tq * sign;
                        let d = q.q - p.q;
                        let dist = d.length();
                        let turn = t.dot(tq).clamp(-1.0, 1.0).acos();
                        let forward = tq.dot(t) > 0.0 && d.dot(t) > 0.0;
                        let good = forward
                            && turn <= MAX_TURN
                            && dist <= STEP_REACH * *h
                            && dist * turn <= 8.0 * SAGITTA_FACTOR * NURBS_FIT_TOL;
                        if good || (at_min && forward) {
                            return Ok(Step {
                                probe: q,
                                t: tq,
                                dist,
                                turn,
                                boundary: false,
                            });
                        }
                        MSG_LOST
                    }
                },
                Corr::Outside if at_min => {
                    let end = self.bisect(p, t, *h)?;
                    let dist = end.q.distance(p.q);
                    return Ok(Step {
                        probe: end,
                        t,
                        dist,
                        turn: 0.0,
                        boundary: true,
                    });
                }
                Corr::Outside | Corr::Failed => MSG_LOST,
                Corr::Singular => MSG_TANGENTIAL,
            };
            if at_min {
                return Err(fail.into());
            }
            *h = (*h * 0.5).max(H_MIN);
        }
    }

    /// Randpunkt zwischen `p` (gültig) und `p + h·t` (jenseits des Rands), auf `BISECT_TOL`.
    /// Gültig heißt hier: Reste höchstens `BISECT_TOL` (jenseits des Rands ist der Abstand zur
    /// Freiformfläche der Abstand zum Rand, der Randpunkt liegt also auf `BISECT_TOL` genau).
    fn bisect(&self, p: &Probe, t: DVec3, h: f64) -> Result<Probe, String> {
        let eps = BISECT_TOL.max(self.res).min(self.eps);
        let (mut lo, mut hi) = (0.0, h);
        let mut best = *p;
        for _ in 0..BISECT_STEPS {
            if hi - lo <= BISECT_TOL {
                break;
            }
            let mid = 0.5 * (lo + hi);
            let pred = p.q + t * mid;
            match self.correct(pred, pred, t, p.hints(), CORRECT_REACH * h, eps)? {
                Corr::Ok(q) => {
                    lo = mid;
                    best = q;
                }
                _ => hi = mid,
            }
        }
        Ok(best)
    }

    /// Liegt `target` auf derselben Schnittkurve wie der Kurvenpunkt `from` (Tangente `t`)?
    /// Korrektur von `from` in die Ebene durch `target` senkrecht zu `t`.
    fn same_curve(
        &self,
        from: DVec3,
        hints: Hints,
        t: DVec3,
        target: DVec3,
    ) -> Result<bool, String> {
        let reach = CORRECT_REACH * from.distance(target).max(H_MIN);
        Ok(
            match self.correct(from, target, t, hints, reach, self.eps)? {
                Corr::Ok(q) => q.q.distance(target) <= SAME_POINT,
                _ => false,
            },
        )
    }

    fn key(&self, p: DVec3) -> CellKey {
        let c = p / self.cell;
        (c.x.floor() as i64, c.y.floor() as i64, c.z.floor() as i64)
    }

    /// Indizes `(Polylinie, Stück)` in den Rasterzellen um `p`.
    fn near_segments(&self, p: DVec3) -> Vec<(usize, usize)> {
        let (x, y, z) = self.key(p);
        let mut out = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let k = (
                        x.saturating_add(dx),
                        y.saturating_add(dy),
                        z.saturating_add(dz),
                    );
                    if let Some(v) = self.grid.get(&k) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out
    }

    fn push_point(&mut self, pid: usize, q: DVec3, hints: Hints) {
        let poly = &mut self.polys[pid];
        let a = poly.pts[poly.pts.len() - 1];
        let s = poly.arc[poly.arc.len() - 1] + a.distance(q);
        let seg = poly.pts.len() - 1;
        poly.pts.push(q);
        poly.arc.push(s);
        poly.hints.push(hints);
        let key = self.key((a + q) * 0.5);
        self.grid.entry(key).or_default().push((pid, seg));
    }

    /// Kommt das neue Stück `a → b` (Bogenlänge `arc_b` am Ende) einer schon verfolgten Kurve
    /// näher als `HIT_FACTOR · h`? Nachbarstücke derselben Verfolgung zählen nicht.
    fn hits(&self, pid: usize, trace: usize, a: DVec3, b: DVec3, arc_b: f64, h: f64) -> bool {
        self.near_segments((a + b) * 0.5).into_iter().any(|(k, j)| {
            let poly = &self.polys[k];
            if poly.trace == trace {
                let gap = if k == pid {
                    arc_b - poly.arc[j + 1]
                } else {
                    arc_b + poly.arc[j]
                };
                if gap < NEIGHBOR_ARC * self.h_max {
                    return false;
                }
            }
            segment_distance(a, b, poly.pts[j], poly.pts[j + 1]) < HIT_FACTOR * h
        })
    }

    /// Verfolgt eine Halbkurve vom Startpunkt in Richtung `sign · (g_a × g_b)`. Liefert die
    /// Punkte, ihre Startwerte und ob die Kurve zum Anfang zurückkehrte.
    fn march(
        &mut self,
        seed: &Probe,
        sign: f64,
        trace: usize,
        budget: &mut Budget,
    ) -> Result<(Vec<DVec3>, Vec<Hints>, bool), String> {
        let pid = self.polys.len();
        self.polys.push(Poly {
            pts: vec![seed.q],
            arc: vec![0.0],
            hints: vec![seed.hints()],
            trace,
        });
        let start = seed.q;
        let mut p = *seed;
        let mut t = self.tangent(&p).ok_or(MSG_TANGENTIAL)? * sign;
        let mut h = self.h_max;
        let max_len = MAX_CURVE_LENGTH_FACTOR * self.diag.max(H_MIN);
        loop {
            budget.steps += 1;
            if budget.steps > MAX_MARCH_STEPS || budget.length > max_len {
                return Err(msg_too_long());
            }
            let st = self.step(&p, t, sign, &mut h)?;
            let q = st.probe;
            let n = self.polys[pid].pts.len();
            // (a) Rückkehr zum Anfang
            if sign > 0.0
                && !st.boundary
                && n >= RETURN_STEPS
                && q.q.distance(start) < RETURN_FACTOR * h
                && self.same_curve(q.q, q.hints(), st.t, start)?
            {
                if st.t.dot(start - q.q) > 0.0 {
                    self.push_point(pid, q.q, q.hints());
                }
                self.push_point(pid, start, seed.hints());
                let poly = &self.polys[pid];
                return Ok((poly.pts.clone(), poly.hints.clone(), true));
            }
            if st.dist > 0.0 {
                // (c) Treffen auf eine andere (oder dieselbe) Kurve
                let arc_b = self.polys[pid].arc[n - 1] + st.dist;
                if self.hits(pid, trace, p.q, q.q, arc_b, h) {
                    return Err(MSG_BRANCH.into());
                }
                budget.length += st.dist;
                self.push_point(pid, q.q, q.hints());
            }
            // (b) Rand der Freiformfläche
            if st.boundary {
                let poly = &self.polys[pid];
                return Ok((poly.pts.clone(), poly.hints.clone(), false));
            }
            // Schrittweite nach Krümmung (Winkel der letzten beiden Tangenten).
            let kappa = if st.dist > 0.0 {
                st.turn / st.dist
            } else {
                0.0
            };
            let by_curvature = if kappa > 0.0 {
                (8.0 * NURBS_FIT_TOL / kappa).sqrt()
            } else {
                self.h_max
            };
            h = by_curvature.clamp(H_MIN, self.h_max).min(MAX_GROWTH * h);
            p = q;
            t = st.t;
        }
    }

    fn trace(&mut self, seed: &Probe, trace: usize) -> Result<Branch, String> {
        let mut budget = Budget::default();
        let (fwd, fh, closed) = self.march(seed, 1.0, trace, &mut budget)?;
        if closed {
            return Ok(Branch {
                pts: fwd,
                hints: fh,
                closed: true,
            });
        }
        let (bwd, bh, _) = self.march(seed, -1.0, trace, &mut budget)?;
        let mut pts: Vec<DVec3> = bwd.into_iter().rev().collect();
        let mut hints: Vec<Hints> = bh.into_iter().rev().collect();
        pts.extend(fwd.into_iter().skip(1));
        hints.extend(fh.into_iter().skip(1));
        Ok(Branch {
            pts,
            hints,
            closed: false,
        })
    }

    /// Liegt der Startpunkt auf einer schon verfolgten Kurve? Je Polylinie, die näher als
    /// `SKIP_FACTOR · h_max` liegt, wird vom nächsten Polylinienpunkt aus geprüft, ob die
    /// Korrektur genau auf den Startpunkt führt (dieselbe Kurve) – eine nahe, aber andere Kurve
    /// wird so nicht übersprungen.
    fn on_traced(&self, s: &Probe) -> Result<bool, String> {
        let radius = SKIP_FACTOR * self.h_max;
        // je Polylinie das nächste Stück
        let mut best: HashMap<usize, (f64, usize, f64)> = HashMap::new();
        for (k, j) in self.near_segments(s.q) {
            let poly = &self.polys[k];
            let (a, b) = (poly.pts[j], poly.pts[j + 1]);
            let (d, f) = point_segment(s.q, a, b);
            if d < radius && best.get(&k).is_none_or(|x| d < x.0) {
                best.insert(k, (d, j, f));
            }
        }
        let mut list: Vec<(usize, (f64, usize, f64))> = best.into_iter().collect();
        list.sort_by(|x, y| x.1.0.total_cmp(&y.1.0).then(x.0.cmp(&y.0)));
        for (k, (d, j, f)) in list {
            if d <= SAME_POINT {
                return Ok(true);
            }
            let poly = &self.polys[k];
            let (a, b) = (poly.pts[j], poly.pts[j + 1]);
            let dir = (b - a).normalize_or_zero();
            if dir == DVec3::ZERO {
                continue;
            }
            let x = a + (b - a) * f;
            let hints = if f < 0.5 {
                poly.hints[j]
            } else {
                poly.hints[j + 1]
            };
            if self.same_curve(x, hints, dir, s.q)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ---- 4. Annähern ----

    /// Annäherung eines Zweigs als Freiformkurve; die Kurve wird an jedem Punkt und zwischen den
    /// Punkten gegen beide Flächen geprüft (`|F| ≤ NURBS_FIT_TOL`), sonst mit kleinerer Schranke
    /// erneut angenähert.
    fn fit_branch(&self, br: &Branch) -> Result<Curve, String> {
        let mut tol = NURBS_FIT_TOL;
        for _ in 0..FIT_ATTEMPTS {
            if let Ok(c) = NurbsCurve::fit(&br.pts, tol)
                && self.verify(&c, br)?
            {
                return Ok(Curve::nurbs(c));
            }
            tol *= FIT_SHRINK;
        }
        Err(MSG_FIT.into())
    }

    fn verify(&self, c: &NurbsCurve, br: &Branch) -> Result<bool, String> {
        let n = br.pts.len();
        let mut chord = Vec::with_capacity(n);
        let mut s = 0.0;
        chord.push(0.0);
        for w in br.pts.windows(2) {
            s += w[0].distance(w[1]);
            chord.push(s);
        }
        if !s.is_finite() || s <= 0.0 {
            return Ok(false);
        }
        let (a, b) = c.domain();
        let at = |x: f64| a + (b - a) * (x / s).clamp(0.0, 1.0);
        for i in 0..n {
            let mut params = vec![chord[i]];
            if i + 1 < n {
                params.push(0.5 * (chord[i] + chord[i + 1]));
            }
            for x in params {
                let p = c.point(at(x));
                match self.probe(p, br.hints[i]) {
                    Ok(pr) if pr.residual() <= NURBS_FIT_TOL => {}
                    Ok(_) | Err(Fail::NoFoot) => return Ok(false),
                    Err(Fail::Numeric) => return Err(MSG_NUMERIC.into()),
                }
            }
        }
        Ok(true)
    }

    // ---- 5. Berührung ----

    /// Nur Berührpunkte: Proben im Kreis um jeden Punkt (auf der Freiformseite) zeigen, ob die
    /// andere Fläche dort ganz anliegt (teilweise zusammenfallend), auf beiden Seiten liegt
    /// (tangentiale Durchdringung) oder nur auf einer (Berührung).
    fn classify_touch(&self, touches: &[Probe]) -> Result<SsiOutcome, String> {
        let a_is_nurbs = self.a.nurbs.is_some();
        let (side, other) = if a_is_nurbs {
            (&self.a, &self.b)
        } else {
            (&self.b, &self.a)
        };
        let delta = CONTACT_PROBE * self.diag;
        for tp in touches.iter().take(MAX_TOUCH_CHECKS) {
            let own = if a_is_nurbs { tp.ea } else { tp.eb };
            let (e1, e2) = own.g.any_orthonormal_pair();
            let (mut pos, mut neg, mut all_on) = (false, false, true);
            for k in 0..CONTACT_DIRECTIONS {
                let ang = TAU * k as f64 / CONTACT_DIRECTIONS as f64;
                let x = tp.q + (e1 * ang.cos() + e2 * ang.sin()) * delta;
                let e = match side.eval(x, own.foot) {
                    Ok(e) => e,
                    Err(Fail::NoFoot) => continue,
                    Err(Fail::Numeric) => return Err(MSG_NUMERIC.into()),
                };
                let s = side.foot_point(x, &e);
                let f = match other.eval(s, None) {
                    Ok(e) => e.f,
                    Err(Fail::NoFoot) => continue,
                    Err(Fail::Numeric) => return Err(MSG_NUMERIC.into()),
                };
                if f.abs() > self.tol.linear {
                    all_on = false;
                    if f > 0.0 {
                        pos = true;
                    } else {
                        neg = true;
                    }
                }
            }
            if all_on {
                return Err(MSG_PARTIAL.into());
            }
            if pos && neg {
                return Err(MSG_TOUCH_CROSSING.into());
            }
        }
        Ok(SsiOutcome::Touch)
    }
}

/// Zweige mit aneinanderliegenden Enden (`< JOIN_TOL`) verbinden; ein offener Zweig, dessen
/// Enden zusammenfallen, wird geschlossen.
fn join(branches: Vec<Branch>) -> Vec<Branch> {
    let (mut done, mut open): (Vec<Branch>, Vec<Branch>) =
        branches.into_iter().partition(|b| b.closed);
    'outer: loop {
        for i in 0..open.len() {
            for j in 0..open.len() {
                if i == j {
                    continue;
                }
                let (ei, sj, ej) = (
                    open[i].pts[open[i].pts.len() - 1],
                    open[j].pts[0],
                    open[j].pts[open[j].pts.len() - 1],
                );
                let forward = ei.distance(sj) < JOIN_TOL;
                let backward = !forward && ei.distance(ej) < JOIN_TOL;
                if forward || backward {
                    let mut other = open.remove(j);
                    let i = if j < i { i - 1 } else { i };
                    if backward {
                        other.pts.reverse();
                        other.hints.reverse();
                    }
                    open[i].pts.extend(other.pts.into_iter().skip(1));
                    open[i].hints.extend(other.hints.into_iter().skip(1));
                    continue 'outer;
                }
            }
        }
        break;
    }
    for mut b in open {
        let n = b.pts.len();
        if n > 2 && b.pts[0].distance(b.pts[n - 1]) < JOIN_TOL {
            b.pts[n - 1] = b.pts[0];
            b.closed = true;
        }
        done.push(b);
    }
    done
}

/// Abstand von `p` zum Stück `a → b` und der Anteil des nächsten Punkts.
fn point_segment(p: DVec3, a: DVec3, b: DVec3) -> (f64, f64) {
    let d = b - a;
    let l2 = d.length_squared();
    let f = if l2 > 0.0 {
        ((p - a).dot(d) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((a + d * f).distance(p), f)
}

/// Kleinster Abstand zweier Strecken.
fn segment_distance(p1: DVec3, q1: DVec3, p2: DVec3, q2: DVec3) -> f64 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    if a <= 0.0 && e <= 0.0 {
        return r.length();
    }
    let (s, t) = if a <= 0.0 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 0.0 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (p1 + d1 * s).distance(p2 + d2 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance_handles_crossing_parallel_and_degenerate_segments() {
        let d = segment_distance(
            DVec3::new(-1.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, -1.0, 0.5),
            DVec3::new(0.0, 1.0, 0.5),
        );
        assert!((d - 0.5).abs() < 1e-15);
        let d = segment_distance(
            DVec3::ZERO,
            DVec3::X,
            DVec3::new(2.0, 1.0, 0.0),
            DVec3::new(3.0, 1.0, 0.0),
        );
        assert!((d - 2f64.sqrt()).abs() < 1e-15);
        let d = segment_distance(DVec3::ZERO, DVec3::ZERO, DVec3::Y, DVec3::Y);
        assert!((d - 1.0).abs() < 1e-15);
    }

    #[test]
    fn branches_with_common_ends_are_joined_and_closed() {
        let br = |pts: Vec<DVec3>| Branch {
            hints: vec![Hints::default(); pts.len()],
            pts,
            closed: false,
        };
        let a = br(vec![DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)]);
        // umgekehrt angehängt: Ende an Ende
        let b = br(vec![DVec3::ZERO, DVec3::Y, DVec3::new(1.0, 1.0, 0.0)]);
        let out = join(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert!(out[0].closed);
        assert_eq!(out[0].pts.len(), 5);
        assert_eq!(out[0].pts[0], out[0].pts[4]);
    }
}
