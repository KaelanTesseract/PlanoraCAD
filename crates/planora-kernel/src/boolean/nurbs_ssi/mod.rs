//! Numerischer Schnittrechner für Flächenpaare, von denen mindestens eine eine Freiformfläche ist
//! (Spec §6, Entscheidungen N3, N4, N7; Festlegungen in Spec §13).
//!
//! Alle Rechnungen laufen über die vorzeichenbehafteten Abstandsfunktionen `F_a`, `F_b` der beiden
//! Flächen; der Gradient ist die Einheitsnormale am Fußpunkt (für analytische und Freiformflächen
//! gleich). Ablauf:
//!
//! 1. **Prüfen:** analytische Flächen müssen gültig sein; fallen die Flächen (teilweise) zusammen?
//! 2. **Startpunkte** ([`seeds`]): Zerlegung in Bézier-Flicken, gerichtete Unterteilung mit
//!    sicherem Ausschluss; ein Kandidat wird erst angenommen, wenn die Normalen beider Seiten über
//!    ihm nicht parallel sein können (dann enthält er keine geschlossene Schleife und keine zwei
//!    Kurvenäste um eine tangentiale Stelle). Newton im Raum auf beide Flächen.
//! 3. **Verfolgen** ([`march`]) in beide Richtungen, Ende bei Rückkehr zum Anfang, am Rand der
//!    Freiformfläche (Bisektion), beim Treffen auf eine verfolgte Kurve oder an einer tangentialen
//!    Stelle (gemeldet) oder an den Grenzen.
//! 4. **Annähern und Berührung** ([`fit`]): Annäherung je Zweig (bei Bedarf in Stücken), geprüft
//!    an allen Punkten und dazwischen gegen beide Flächen; Einordnung reiner Berührpunkte.
//!
//! Nicht lösbare Lagen ergeben [`SsiOutcome::Unsupported`] mit deutscher Meldung, nie ein stilles
//! Teilergebnis. Alle Schleifen haben harte Grenzen; zusätzlich begrenzt `MAX_PROBES` die Zahl
//! der Flächenauswertungen insgesamt.

mod edges;
mod fit;
mod march;
mod seeds;

use std::cell::Cell;
use std::collections::HashMap;

use crate::geometry::{Curve, Surface};
use crate::nurbs_surface::NurbsSurfaceData;
use planora_math::{DVec3, Frame, Tolerance};

use march::{Branch, Poly};

/// Fehlerschranke der Annäherung einer Schnittkurve (N4/Q5) in mm.
pub const NURBS_FIT_TOL: f64 = 1e-5;
/// Größte Unterteilungstiefe der Flicken bei der Startpunktsuche (je Parameterrichtung).
pub const MAX_SUBDIVISION_DEPTH: usize = 12;
/// Höchstzahl Startpunkte je Flächenpaar.
pub const MAX_SEEDS: usize = 512;
/// Höchstzahl Schritte je Verfolgung (beide Richtungen zusammen).
pub const MAX_MARCH_STEPS: usize = 20_000;
/// Größte Kurvenlänge = Faktor · Diagonale der Hüllquader beider Flächen.
pub const MAX_CURVE_LENGTH_FACTOR: f64 = 20.0;

/// Höchstzahl Auswertungen beider Abstandsfunktionen (Punktproben) je Flächenpaar: begrenzt die
/// Rechenzeit insgesamt, auch über Wiederholungen hinweg.
const MAX_PROBES: usize = 120_000;
/// Konvergenz: `|F_a|, |F_b| ≤ CONVERGE_REL · max(Diagonale, 1 mm)`, mindestens
/// `COORD_ULPS · ε · größter Koordinatenbetrag` (Auflösung von f64 fern vom Ursprung).
const CONVERGE_REL: f64 = 1e-9;
const COORD_ULPS: f64 = 64.0;
/// Zwei Punkte (Startpunkte, Kurvenenden, Prüfung „dieselbe Kurve“) gelten als gleich, wenn sie
/// höchstens `max(tol.linear, SAME_REL · Diagonale)` auseinander liegen (das Hundertfache der
/// Konvergenzschranke; bei großen Teilen wächst die Schranke mit).
const SAME_REL: f64 = 1e-7;
/// `|g_a × g_b|` darunter: Normalen parallel (tangentiale Berührung).
const PARALLEL_NORMALS: f64 = 1e-4;
/// Größte Schrittweite = Diagonale / `STEP_DIVISOR`.
const STEP_DIVISOR: f64 = 200.0;
/// Kleinste Schrittweite (mm).
const H_MIN: f64 = 1e-4;
/// Sehnenfehler der Polylinie je Schritt (mm), mindestens; mit der Größe wachsend
/// (`MARCH_SAGITTA_REL · Diagonale`). Die Kurve selbst hält `NURBS_FIT_TOL` über die geprüfte
/// Annäherung ein (die kubische Annäherung ist zwischen den Punkten weit genauer als die Sehne);
/// die Polylinie muss nur dicht genug für Verfolgung und Annäherung sein.
const MARCH_SAGITTA: f64 = 1e-3;
const MARCH_SAGITTA_REL: f64 = 1e-6;
/// Kleinster erlaubter Radius analytischer Flächen (mm).
const MIN_RADIUS: f64 = 1e-4;
/// Toleranz der Orthonormalität eines Rahmens.
const FRAME_EPS: f64 = 1e-9;

const MSG_NUMERIC: &str = "Numerischer Fehler beim Flächenschnitt (ungültiger Zahlenwert)";
const MSG_TANGENTIAL: &str =
    "Die Flächen berühren sich tangential; der Schnitt lässt sich dort nicht sicher bestimmen";
const MSG_NEAR_TANGENT: &str = "Die Flächen liegen stellenweise nahezu tangential zueinander; \
     der Schnitt lässt sich dort nicht sicher bestimmen";
const MSG_TOUCH_AND_CUT: &str =
    "Die Flächen berühren sich tangential an einer Stelle und schneiden sich an einer anderen";
const MSG_TOUCH_CROSSING: &str = "Die Flächen berühren sich tangential und durchdringen sich dabei";
const MSG_BRANCH: &str = "Die Schnittkurve verzweigt sich oder trifft eine andere Schnittkurve";
const MSG_SINGULAR: &str = "Die Schnittkurve verzweigt sich an einer Stelle, an der sich die \
     Flächen tangential berühren";
const MSG_LOST: &str =
    "Die Schnittkurve lässt sich nicht verfolgen (Schrittweite unter der Grenze)";
const MSG_FIT: &str = "Die Schnittkurve lässt sich nicht annähern";
const MSG_PARTIAL: &str = "Die Flächen fallen teilweise zusammen";
const MSG_NEARLY_COINCIDENT: &str =
    "Die Flächen liegen fast aufeinander; der Schnitt lässt sich nicht sicher bestimmen";
/// Fast zusammenfallend: alle Stichproben näher als `NEAR_REL · Diagonale` an der anderen
/// Fläche (aber nicht innerhalb `tol.linear`).
const NEAR_REL: f64 = 1e-4;

fn msg_too_many_seeds() -> String {
    format!("Zu viele Schnittbereiche (mehr als {MAX_SEEDS} Startpunkte)")
}

fn msg_budget() -> String {
    format!("Der Flächenschnitt ist zu aufwendig (mehr als {MAX_PROBES} Auswertungen der Flächen)")
}

fn msg_too_many_steps() -> String {
    format!("Die Schnittkurve braucht zu viele Schritte (mehr als {MAX_MARCH_STEPS})")
}

fn msg_too_long() -> String {
    format!(
        "Die Schnittkurve ist zu lang (länger als das {MAX_CURVE_LENGTH_FACTOR}-Fache der \
         Flächengröße)"
    )
}

/// Ergebnis des numerischen Schnitts.
#[derive(Clone, Debug)]
pub enum SsiOutcome {
    /// Schnittkurven als angenäherte Freiformkurven. Geschlossene Kurven haben Anfang = Ende
    /// exakt, aber an dieser Naht nur Stetigkeit C0 (die Annäherung ist eine offene Kurve).
    /// Sehr lange Zweige können in mehrere Kurven geteilt sein, deren Enden aufeinanderfallen.
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
    if let Err(msg) = check_surface(a).and_then(|_| check_surface(b)) {
        return SsiOutcome::Unsupported(msg);
    }
    let mut ssi = Ssi::new(a, b, *tol);
    ssi.run().unwrap_or_else(SsiOutcome::Unsupported)
}

/// Gültigkeit einer analytischen Fläche: endlicher, orthonormaler Rahmen, endliche Maße, Radien
/// mindestens `MIN_RADIUS` (Freiformflächen prüft ihr Konstruktor).
fn check_surface(s: &Surface) -> Result<(), String> {
    let bad = |what: &str| Err(format!("Ungültige Fläche für den Schnitt: {what}"));
    let frame_ok = |f: &Frame| f.is_orthonormal(FRAME_EPS);
    let radius_ok = |r: f64| r.is_finite() && r >= MIN_RADIUS;
    match s {
        Surface::Nurbs(_) => Ok(()),
        _ if !frame_ok(s.frame()) => bad("das Koordinatensystem ist nicht orthonormal"),
        Surface::Plane { .. } => Ok(()),
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => {
            if radius_ok(*radius) {
                Ok(())
            } else {
                bad(&format!("Radius {radius} (mindestens {MIN_RADIUS} mm)"))
            }
        }
        Surface::Cone { radius, slope, .. } => {
            if radius.is_finite() && *radius >= 0.0 && slope.is_finite() {
                Ok(())
            } else {
                bad("Kegel mit ungültigem Radius oder ungültiger Steigung")
            }
        }
        Surface::Torus { major, minor, .. } => {
            if radius_ok(*minor) && major.is_finite() && *major > *minor {
                Ok(())
            } else {
                bad("Torus mit ungültigen Radien")
            }
        }
    }
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

/// Auswertung gescheitert.
enum Fail {
    /// Die Fußpunktsuche fand nichts (oder die Normale fehlt): weicher Fehlschlag.
    NoFoot,
    /// NaN oder unendlich: harter Fehler.
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

type CellKey = (i64, i64, i64);

struct Ssi<'a> {
    a: Side<'a>,
    b: Side<'a>,
    tol: Tolerance,
    diag: f64,
    eps: f64,
    /// Auflösung der Koordinaten (siehe `COORD_ULPS`).
    res: f64,
    /// Gleichheit zweier Punkte (siehe `SAME_REL`).
    same: f64,
    /// Sehnenfehler je Schritt (siehe `MARCH_SAGITTA`).
    sagitta: f64,
    h_max: f64,
    polys: Vec<Poly>,
    grid: HashMap<CellKey, Vec<(usize, usize)>>,
    cell: f64,
    visits: usize,
    edge_visits: usize,
    candidates: usize,
    probes: Cell<usize>,
    /// Größte Diagonale eines Kandidaten-Flickens.
    max_patch: f64,
    /// Gezählte Startpunkte, Berührpunkte, Zweige.
    starts: Vec<DVec3>,
    touches: Vec<Probe>,
    branches: Vec<Branch>,
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
            same: tol.linear.max(SAME_REL * diag),
            sagitta: MARCH_SAGITTA.max(MARCH_SAGITTA_REL * diag),
            h_max,
            polys: Vec::new(),
            grid: HashMap::new(),
            cell: march::CELL_FACTOR * h_max,
            visits: 0,
            edge_visits: 0,
            candidates: 0,
            probes: Cell::new(0),
            max_patch: seeds::max_patch(a, b, diag),
            starts: Vec::new(),
            touches: Vec::new(),
            branches: Vec::new(),
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
        if shares.iter().all(|s| *s >= seeds::COINCIDENT_SHARE) {
            return Ok(SsiOutcome::Coincident);
        }
        if shares.iter().any(|s| *s >= seeds::PARTIAL_SHARE) {
            return Err(MSG_PARTIAL.into());
        }
        // Fast zusammenfallend (überall dichter als `NEAR_REL · Diagonale`): Der Schnitt wäre
        // schlecht bestimmt und die Unterteilung fände kein Ende.
        let near = NEAR_REL * self.diag;
        let near_shares: Vec<f64> = [
            self.share_within(&self.a, &self.b, near),
            self.share_within(&self.b, &self.a, near),
        ]
        .into_iter()
        .flatten()
        .collect();
        if near_shares.iter().all(|s| *s >= seeds::COINCIDENT_SHARE) {
            return Err(MSG_NEARLY_COINCIDENT.into());
        }
        // Startpunkte und Verfolgung verschränkt (siehe `seeds::Ssi::search`).
        self.search()?;
        let branches = march::join(std::mem::take(&mut self.branches), self.same);
        if !branches.is_empty() {
            if !self.touches.is_empty() {
                return Err(MSG_TOUCH_AND_CUT.into());
            }
            let mut curves = Vec::new();
            for b in &branches {
                curves.extend(self.fit_branch(b)?);
            }
            return Ok(SsiOutcome::Curves(curves));
        }
        if !self.touches.is_empty() {
            return Ok(SsiOutcome::Touch);
        }
        Ok(SsiOutcome::None)
    }

    /// Beide Abstandsfunktionen an `q`. `Ok(None)`: Fußpunktsuche gescheitert (weich);
    /// `Err`: Zahlenfehler oder Rechenbudget erschöpft.
    fn probe(&self, q: DVec3, h: Hints) -> Result<Option<Probe>, String> {
        self.spend()?;
        let lift = |r: Result<Eval, Fail>| match r {
            Ok(e) => Ok(Some(e)),
            Err(Fail::NoFoot) => Ok(None),
            Err(Fail::Numeric) => Err(String::from(MSG_NUMERIC)),
        };
        let Some(ea) = lift(self.a.eval(q, h.a))? else {
            return Ok(None);
        };
        let Some(eb) = lift(self.b.eval(q, h.b))? else {
            return Ok(None);
        };
        Ok(Some(Probe { q, ea, eb }))
    }

    /// Eine Seite allein an `q` (zählt wie eine Probe).
    fn eval_side(&self, side: &Side, q: DVec3, hint: Option<Uv>) -> Result<Option<Eval>, String> {
        self.spend()?;
        match side.eval(q, hint) {
            Ok(e) => Ok(Some(e)),
            Err(Fail::NoFoot) => Ok(None),
            Err(Fail::Numeric) => Err(MSG_NUMERIC.into()),
        }
    }

    fn spend(&self) -> Result<(), String> {
        let n = self.probes.get() + 1;
        self.probes.set(n);
        if n > MAX_PROBES {
            return Err(msg_budget());
        }
        Ok(())
    }

    fn tangent(&self, p: &Probe) -> Option<DVec3> {
        let c = p.cross();
        let l = c.length();
        (l >= PARALLEL_NORMALS && l.is_finite()).then(|| c / l)
    }
}
