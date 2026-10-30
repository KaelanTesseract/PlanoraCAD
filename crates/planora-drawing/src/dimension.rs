//! Datentypen für Maße (Teilprojekt 12): Verweise auf Modellgeometrie, Maßarten, Toleranzen,
//! Maßketten und die Fehler einer Maßangabe; die Messung ([`measure`]) aus aufgelöster
//! Modellgeometrie ([`Geo3`]) und der Ansichtsbasis.

use crate::basis::Basis;
use crate::limits::geom_tol;
use crate::model::{ViewId, ViewKind};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3};
use std::f64::consts::TAU;

/// Kennung eines Maßes innerhalb einer Zeichnung (nie wiederverwendet).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DimId(pub u64);

/// Art eines Maßes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimKind {
    Horizontal,
    Vertical,
    Aligned,
    Diameter,
    Radius,
    Angle,
}

/// Anfang oder Ende einer Kante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndSel {
    Start,
    End,
}

/// Marke einer Fläche: Herkunft, Rolle und Hinweispunkt (wie im Modell, aber ohne
/// Abhängigkeit von `planora-feature`).
#[derive(Clone, Debug, PartialEq)]
pub struct TagData {
    pub origin: u64,
    pub role: String,
    pub hint: DVec3,
}

/// Verweis auf Modellgeometrie ohne Abhängigkeit von `planora-feature`.
#[derive(Clone, Debug, PartialEq)]
pub enum GeoRefData {
    /// Fläche (Zylinder, Kugel, Ebene …).
    Face(TagData),
    /// Kante über die zwei angrenzenden Flächen.
    Edge { a: TagData, b: TagData, hint: DVec3 },
}

/// Ein Bezug eines Maßes.
#[derive(Clone, Debug, PartialEq)]
pub enum GeoRef {
    Edge(GeoRefData),
    EdgeEnd { edge: GeoRefData, end: EndSel },
    Center(GeoRefData),
    Face(GeoRefData),
}

/// Toleranzangabe eines Maßes.
#[derive(Clone, Debug, PartialEq)]
pub enum Tolerance {
    None,
    Symmetric(f64),
    Limits {
        upper: f64,
        lower: f64,
    },
    /// Grenzmaße: `upper`/`lower` sind **Abweichungen** vom Nennmaß (wie bei `Limits`); gezeigt
    /// werden Höchst- und Mindestmaß `Nennmaß + upper` / `Nennmaß + lower` statt des Nennmaßes,
    /// z. B. 12,5 mit +0,1/−0,1 → „12,6“ und „12,4“.
    LimitSizes {
        upper: f64,
        lower: f64,
    },
    Fit(String),
}

/// Maßkette (gemeinsame Maßlinie) oder Bezugsmaße (gestapelte Maßlinien).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainMode {
    Chain,
    Baseline,
}

/// Zugehörigkeit eines Maßes zu einer Gruppe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainSpec {
    pub group: u64,
    pub mode: ChainMode,
    pub order: u32,
}

/// Ein Maß einer Zeichnung.
#[derive(Clone, Debug, PartialEq)]
pub struct DimSpec {
    pub id: DimId,
    pub view: ViewId,
    pub kind: DimKind,
    pub refs: Vec<GeoRef>,
    /// Abstand der Maßlinie in Blatt-mm.
    pub offset: f64,
    /// Winkel der Führungslinie in Grad (nur Ø/R).
    pub angle: f64,
    /// Verschiebung des Textes in Blatt-mm.
    pub text_shift: (f64, f64),
    pub tolerance: Tolerance,
    pub decimals: u8,
    pub text_override: Option<String>,
    pub chain: Option<ChainSpec>,
    pub suggested: bool,
}
/// Bogen einer Kreiskante im Raum: Anfangs-, Mittel- und Endpunkt (auf dem Kreis). Die
/// Reihenfolge von Anfang und Ende ist gleichgültig; der Mittelpunkt legt fest, welcher der
/// beiden Bögen zwischen ihnen gemeint ist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc3 {
    pub start: DVec3,
    pub mid: DVec3,
    pub end: DVec3,
}

/// Aufgelöste Modellgeometrie eines Bezugs.
#[derive(Clone, Debug, PartialEq)]
pub enum Geo3 {
    Point(DVec3),
    Line {
        a: DVec3,
        b: DVec3,
    },
    /// Kreis oder Bogen: `arc` ist `None` für einen vollen Kreis, sonst der Bogen der Kante
    /// (wird beim Auflösen aus dem Parameterbereich der Kante bestimmt, nie gespeichert).
    Circle {
        center: DVec3,
        normal: DVec3,
        radius: f64,
        arc: Option<Arc3>,
    },
    /// Zylinderfläche: Achse durch `origin` mit der Richtung `dir` (wird normiert), Radius und
    /// der gezeichnete Bereich der Fläche entlang der Achse als Parameter `t0 ≤ t1` entlang der
    /// **normierten** Richtung ab `origin`: Enden `origin + t0·dir/|dir|` und
    /// `origin + t1·dir/|dir|`. `planora-feature` (Aufgabe 8) muss `t0`/`t1` aus der Ausdehnung
    /// der Fläche entlang der Achse füllen.
    Axis {
        origin: DVec3,
        dir: DVec3,
        radius: f64,
        t0: f64,
        t1: f64,
    },
    Sphere {
        center: DVec3,
        radius: f64,
    },
}

/// Fehler einer Maßangabe. Der Text entsteht in der aktuellen Sprache; `Lost` und `Invalid`
/// tragen einen Grund, der beim Entstehen in der damals aktuellen Sprache geschrieben wurde.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
#[error("{}", self.text())]
pub enum DimError {
    NotAcross,
    NotVisible,
    Ambiguous,
    Lost(String),
    NotAllowedInIsometric,
    TooMany,
    Invalid(String),
}

impl DimError {
    /// Die Meldung in der aktuellen Sprache.
    fn text(&self) -> String {
        match self {
            DimError::NotAcross => tr!("drawing-dim-err-not-across"),
            DimError::NotVisible => tr!("drawing-dim-err-not-visible"),
            DimError::Ambiguous => tr!("drawing-dim-err-ambiguous"),
            DimError::Lost(why) => tr!("drawing-dim-err-lost", reason = why.as_str()),
            DimError::NotAllowedInIsometric => tr!("drawing-dim-err-isometric"),
            DimError::TooMany => tr!("drawing-dim-err-too-many"),
            DimError::Invalid(why) => tr!("drawing-dim-err-invalid", reason = why.as_str()),
        }
    }
}

/// Ergebnis einer Messung in **Ansichtskoordinaten** (ungeskaliert, mm; y nach oben).
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// Lineares Maß (Horizontal/Vertical/Aligned und Ø quer): Messrichtung `dir`
    /// (Einheitsvektor), Wert `|(b − a)·dir|`. Querachse `w` = `dir` um 90° gedreht, nach rechts
    /// bzw. bei senkrechter Lage nach oben gerichtet (Waagerecht: `w = (0, 1)`, Senkrecht:
    /// `w = (1, 0)`). `a_span`/`b_span`: die beiden Enden des ersten/zweiten Bezugs quer zur
    /// Messrichtung, geordnet `(kleinere, größere Querlage)`; bei einem punktförmigen Bezug
    /// zweimal derselbe Punkt. `a`/`b` (zur Verträglichkeit beibehalten): bei Waagerecht,
    /// Senkrecht und Ø quer das Ende mit der größeren Querlage (`a_span.1`/`b_span.1`), bei
    /// Schräg die Messpunkte (siehe [`measure`]). Die Darstellung nimmt je Bezug das Ende, das
    /// der Maßlinie am nächsten liegt.
    Linear {
        a: DVec2,
        b: DVec2,
        dir: DVec2,
        a_span: (DVec2, DVec2),
        b_span: (DVec2, DVec2),
    },
    /// Kreis mit Blick in Achsrichtung (oder Kugel): Mittelpunkt, Radius und – bei einem Bogen –
    /// dessen Lage im Bild (`None`: voller Kreis).
    Round {
        center: DVec2,
        radius: f64,
        arc: Option<ArcSpan>,
    },
    /// Winkel zwischen zwei Geraden: Scheitel und zwei Einheitsrichtungen vom Scheitel weg.
    Angle { vertex: DVec2, d1: DVec2, d2: DVec2 },
}

/// Ein Winkel liegt auf einem Bogen ([`ArcSpan::contains`]), wenn er höchstens so weit
/// (Radiant) außerhalb liegt.
pub const ARC_ANGLE_TOL: f64 = 1e-9;

/// Lage eines Bogens im Bild (und auf dem Blatt, das nicht dreht): Anfangswinkel `start` in
/// `[0, 2π)` und Überstrich `sweep` in `(0, 2π)` gegen den Uhrzeigersinn (Radiant).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcSpan {
    pub start: f64,
    pub sweep: f64,
}

impl ArcSpan {
    /// Bogen durch die Bildpunkte `a` (Anfang oder Ende), `m` (dazwischen) und `b` um `c`;
    /// `None`, wenn ein Punkt auf der Mitte liegt, eine Zahl nicht endlich ist oder die Punkte
    /// (fast) den vollen Kreis umfassen.
    pub fn through(c: DVec2, a: DVec2, m: DVec2, b: DVec2) -> Option<ArcSpan> {
        let ang = |p: DVec2| {
            let v = p - c;
            (v.length() > 0.0 && v.is_finite()).then(|| v.y.atan2(v.x).rem_euclid(TAU))
        };
        let (a0, am, a1) = (ang(a)?, ang(m)?, ang(b)?);
        let ccw = (a1 - a0).rem_euclid(TAU);
        let (start, sweep) = if (am - a0).rem_euclid(TAU) <= ccw {
            (a0, ccw)
        } else {
            (a1, TAU - ccw)
        };
        let ok = start.is_finite() && sweep > ARC_ANGLE_TOL && sweep < TAU - ARC_ANGLE_TOL;
        ok.then_some(ArcSpan { start, sweep })
    }

    /// Liegt der Winkel `t` (Radiant, beliebig) auf dem Bogen (bis auf [`ARC_ANGLE_TOL`])?
    pub fn contains(&self, t: f64) -> bool {
        let d = (t - self.start).rem_euclid(TAU);
        d <= self.sweep + ARC_ANGLE_TOL || d >= TAU - ARC_ANGLE_TOL
    }

    /// Winkel der Bogenmitte (Radiant).
    pub fn mid(&self) -> f64 {
        self.start + 0.5 * self.sweep
    }

    /// Endwinkel `start + sweep` (Radiant).
    pub fn end(&self) -> f64 {
        self.start + self.sweep
    }

    /// `t` selbst, wenn er auf dem Bogen liegt, sonst das nähere Bogenende (Radiant; bei
    /// gleichem Abstand der Anfang).
    pub fn clamp(&self, t: f64) -> f64 {
        if self.contains(t) {
            return t;
        }
        let d = (t - self.start).rem_euclid(TAU);
        // Außerhalb: d ∈ (sweep, 2π). Abstand zum Ende d − sweep, zum Anfang 2π − d.
        if d - self.sweep < TAU - d {
            self.end()
        } else {
            self.start
        }
    }
}

/// Gemessener Wert (Modell-mm, Winkel in Grad), Anker und Vorsatzzeichen (`'Ø'` oder `'R'`).
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub value: f64,
    pub anchor: Anchor,
    pub prefix: Option<char>,
}

/// Querprüfung (relativ): Eine Richtung `v` im Bild steht quer zur Messrichtung `u`, wenn
/// `|v·u| ≤ ACROSS_TOL·|v|`; ebenso gelten zwei Kanten für ein schräges Maß als parallel, wenn
/// der Sinus ihres Winkels höchstens `ACROSS_TOL` ist.
pub const ACROSS_TOL: f64 = 1e-6;
/// Blickprüfung für Kreise und Achsen (Einheitsvektoren `n`, `d`): Blick in Achsrichtung bei
/// `|n·d| ≥ 1 − VIEW_TOL`, von der Seite bei `|n·d| ≤ VIEW_TOL`; dazwischen schräg.
pub const VIEW_TOL: f64 = 1e-9;
/// Kanten eines Winkelmaßes gelten als parallel, wenn der Sinus ihres Winkels im Bild höchstens
/// `PARALLEL_SIN` ist.
pub const PARALLEL_SIN: f64 = 1e-9;
/// Ein Scheitel liegt am Ende einer Kante, wenn sein Kantenparameter (0 am Anfang, 1 am Ende)
/// höchstens so weit innerhalb der Kante liegt.
pub const END_PARAM_TOL: f64 = 1e-6;
/// Kanten eines Winkelmaßes müssen parallel zur Bildebene liegen: `|e·d| ≤ ANGLE_PLANE_TOL`
/// (`e` normierte Kantenrichtung im Raum, `d` Blickrichtung).
pub const ANGLE_PLANE_TOL: f64 = 1e-6;

/// Gründe für [`DimError::Invalid`] (Text in der aktuellen Sprache).
#[derive(Clone, Copy)]
enum Why {
    ZeroLength,
    Oblique,
    NotFinite,
    WrongCount,
    AlignedRefs,
    RoundRefs,
    AngleRefs,
    NoRadius,
    NoDirection,
    PointEdge,
    Parallel,
    AxisRange,
    RadiusSide,
    RadiusAxis,
    AngleOblique,
}

fn invalid(why: Why) -> DimError {
    DimError::Invalid(match why {
        Why::ZeroLength => tr!("drawing-dim-zero-length"),
        Why::Oblique => tr!("drawing-dim-oblique"),
        Why::NotFinite => tr!("drawing-dim-not-finite"),
        Why::WrongCount => tr!("drawing-dim-wrong-count"),
        Why::AlignedRefs => tr!("drawing-dim-aligned-refs"),
        Why::RoundRefs => tr!("drawing-dim-round-refs"),
        Why::AngleRefs => tr!("drawing-dim-angle-refs"),
        Why::NoRadius => tr!("drawing-dim-no-radius"),
        Why::NoDirection => tr!("drawing-dim-no-direction"),
        Why::PointEdge => tr!("drawing-dim-point-edge"),
        Why::Parallel => tr!("drawing-dim-parallel"),
        Why::AxisRange => tr!("drawing-dim-axis-range"),
        Why::RadiusSide => tr!("drawing-dim-radius-side"),
        Why::RadiusAxis => tr!("drawing-dim-radius-axis"),
        Why::AngleOblique => tr!("drawing-dim-angle-oblique"),
    })
}

fn geo_is_finite(g: &Geo3) -> bool {
    match g {
        Geo3::Point(p) => p.is_finite(),
        Geo3::Line { a, b } => a.is_finite() && b.is_finite(),
        Geo3::Circle {
            center,
            normal,
            radius,
            arc,
        } => {
            center.is_finite()
                && normal.is_finite()
                && radius.is_finite()
                && arc.is_none_or(|a| a.start.is_finite() && a.mid.is_finite() && a.end.is_finite())
        }
        Geo3::Axis {
            origin,
            dir,
            radius,
            t0,
            t1,
        } => {
            origin.is_finite()
                && dir.is_finite()
                && radius.is_finite()
                && t0.is_finite()
                && t1.is_finite()
        }
        Geo3::Sphere { center, radius } => center.is_finite() && radius.is_finite(),
    }
}

fn span_is_finite(s: &(DVec2, DVec2)) -> bool {
    s.0.is_finite() && s.1.is_finite()
}

fn anchor_is_finite(a: &Anchor) -> bool {
    match a {
        Anchor::Linear {
            a,
            b,
            dir,
            a_span,
            b_span,
        } => {
            a.is_finite()
                && b.is_finite()
                && dir.is_finite()
                && span_is_finite(a_span)
                && span_is_finite(b_span)
        }
        Anchor::Round {
            center,
            radius,
            arc,
        } => {
            center.is_finite()
                && radius.is_finite()
                && arc.is_none_or(|a| a.start.is_finite() && a.sweep.is_finite())
        }
        Anchor::Angle { vertex, d1, d2 } => vertex.is_finite() && d1.is_finite() && d2.is_finite(),
    }
}

/// Bild einer Kante: Endpunkte und Richtung `proj(b − a)` (Differenz im Raum gebildet, dann
/// projiziert: genauer als die Differenz der Bildpunkte bei großen Koordinaten).
struct Seg {
    a: DVec2,
    b: DVec2,
    v: DVec2,
    len: f64,
}

impl Seg {
    fn of(basis: &Basis, a: DVec3, b: DVec3) -> Result<Seg, DimError> {
        let v = basis.project(b - a);
        let s = Seg {
            a: basis.project(a),
            b: basis.project(b),
            v,
            len: v.length(),
        };
        if s.a.is_finite() && s.b.is_finite() && s.v.is_finite() && s.len.is_finite() {
            Ok(s)
        } else {
            Err(invalid(Why::NotFinite))
        }
    }
}

/// Enden `origin + t0·e`, `origin + t1·e` einer Zylinderachse (`e` normierte Richtung).
fn axis_ends(
    origin: DVec3,
    dir: DVec3,
    t0: f64,
    t1: f64,
) -> Result<(DVec3, DVec3, DVec3), DimError> {
    let e = unit3(dir)?;
    let (p0, p1) = (origin + e * t0, origin + e * t1);
    if p0.is_finite() && p1.is_finite() {
        Ok((e, p0, p1))
    } else {
        Err(invalid(Why::NotFinite))
    }
}

/// Misst ein Maß der Art `kind` an den aufgelösten Bezügen `geos` in der Ansicht mit der Basis
/// `basis` (wie von [`Basis::of`]: orthonormal, `ex` rechts, `ey` oben). `extent` ist die
/// Ausdehnung des Modells (größte Kante der Hüllbox); daraus `tol = geom_tol(extent)`.
/// Der Wert hängt nur von der Basis ab, nicht vom Blattmaßstab (Modell-mm bzw. Grad). Längen
/// werden **im Bild** gemessen, wie in Zeichnungen üblich: Ein schräges Maß zwischen zwei
/// Punkten verschiedener Tiefe zeigt den Abstand ihrer Bildpunkte.
///
/// Regeln:
/// - `view_kind == Isometric` → [`DimError::NotAllowedInIsometric`] (vor allen anderen Prüfungen).
/// - Falsche Bezugszahl ([`ref_count`]), nicht endliche Zahlen in Bezügen, Basis oder `extent`
///   (oder `extent < 0`), Zylinderbereich mit `t1 < t0`, Überlauf beim Projizieren → `Invalid`
///   (in dieser Reihenfolge, vor den fachlichen Prüfungen). Nie ein Absturz.
/// - **Horizontal/Vertical:** Messrichtung `u = (1, 0)` (entlang `ex`) bzw. `(0, 1)` (entlang
///   `ey`), Querachse `w` die andere Bildachse. Lage je Bezug entlang `u`: `Point`, `Circle`
///   (Mitte), `Sphere` (Mitte) → Bildpunkt (Spanne: zweimal der Punkt); `Line` mit Bildlänge
///   `≤ 16·tol` (Kante entlang der Blickrichtung, wie in `compute_view` verworfen) → Bildmitte
///   als Punkt; sonst muss die Bildrichtung `v` quer stehen (`|v·u| ≤ ACROSS_TOL·|v|`, sonst
///   `NotAcross`), Lage = Mittel der beiden Enden, Spanne = die beiden Enden mit dieser Lage
///   entlang `u`, geordnet nach Querlage; `Axis` wie eine Kante zwischen ihren Enden
///   `origin + t0·e`, `origin + t1·e` (mit `|e·d| ≥ 1 − VIEW_TOL` als Punkt, sonst Querprüfung
///   mit der Bildrichtung von `e`). `a`/`b` = Ende mit der größeren Querlage (Waagerecht: oben,
///   Senkrecht: rechts). Wert `|lage_b − lage_a|`; `< tol` → `Invalid("Das Maß hat die Länge
///   null.")`. `dir = u`.
/// - **Aligned:** zwei Punkte (`Point` oder als Punkt gesehene `Line`): `dir = (b − a)/|b − a|`,
///   Spannen je zweimal der Punkt. Oder zwei Kanten, deren Bildrichtungen parallel sind (Sinus
///   `≤ ACROSS_TOL`): `e` = Bildrichtung der ersten, `n = e⊥`; Bereiche beider Kanten entlang
///   `e`; überlappen sie, liegen `a` (auf Kante 1) und `b` (Fußpunkt auf Gerade 2) bei der Mitte
///   der Überlappung, sonst an den einander nächsten Enden. Wert `|(b − a)·n|` (Abstand der
///   Geraden), `dir = ±n` von a nach b; Spannen = die Bildenden der Kanten. Alles andere →
///   `Invalid`.
/// - **Diameter/Radius:** `Circle` mit `|n·d| ≥ 1 − VIEW_TOL` → `Round` (Mitte im Bild, `r`,
///   bei einem Bogen dessen Lage im Bild aus den Bildern seiner drei Punkte, [`ArcSpan::through`];
///   sonst und bei Zylinder und Kugel `arc = None`), Wert `2r`/`r`, Vorsatz `'Ø'`/`'R'`; mit `|n·d| ≤ VIEW_TOL` (von der Seite) → Ø als
///   `Linear` quer über die Bildstrecke (`a = c − r·t`, `b = c + r·t`, `t` Einheitsrichtung der
///   Strecke, Spannen je zweimal der Punkt), Radius →
///   `Invalid("Der Radius ist nur mit Blick auf den Kreis möglich.")`; `Axis` mit
///   `|e·d| ≤ VIEW_TOL` → Ø als `Linear` quer zur Bildachse: Umrisslinien `c(t) ± r·t` zwischen
///   den Bildern der Enden, Spannen = deren Enden, `a`/`b` = Ende mit größerer Querlage;
///   `Axis` mit `|e·d| ≥ 1 − VIEW_TOL` → Ø als `Round`; Radius an einer Zylinderfläche →
///   `Invalid`; `Sphere` → `Round`. Sonst `Invalid("Der Kreis erscheint schräg.")`. Die
///   Richtung `t` eines linearen Ø zeigt nach rechts, bei senkrechter Lage (`|t.x| ≤ 1e-9`)
///   nach oben.
/// - **Angle:** zwei `Line` mit Bildlänge `> 16·tol`, beide parallel zur Bildebene
///   (`|e·d| ≤ ANGLE_PLANE_TOL`, sonst `Invalid`: der Winkel erschiene verzerrt), Sinus des
///   Winkels der Bildgeraden `> PARALLEL_SIN` (sonst `Invalid`). Scheitel = Schnitt der
///   unbegrenzten Bildgeraden. Richtung je Kante: Liegt der Scheitel (Kantenparameter `τ`, 0 am
///   Anfang, 1 am Ende) bis auf `END_PARAM_TOL` an einem Ende oder außerhalb, zeigt sie vom
///   Scheitel zur Kante hin; liegt er innerhalb, zeigt sie zum näheren Ende (`τ < 0,5` →
///   Anfang, sonst Ende). Wert = `atan2(|d1 × d2|, d1·d2)` in Grad, im offenen Bereich
///   (0°, 180°).
pub fn measure(
    kind: DimKind,
    geos: &[Geo3],
    basis: &Basis,
    view_kind: ViewKind,
    extent: f64,
) -> Result<Measured, DimError> {
    if view_kind == ViewKind::Isometric {
        return Err(DimError::NotAllowedInIsometric);
    }
    let (lo, hi) = ref_count(kind);
    if geos.len() < lo || geos.len() > hi {
        return Err(invalid(Why::WrongCount));
    }
    let basis_ok = basis.d.is_finite() && basis.ex.is_finite() && basis.ey.is_finite();
    if !(basis_ok && extent.is_finite() && extent >= 0.0 && geos.iter().all(geo_is_finite)) {
        return Err(invalid(Why::NotFinite));
    }
    // Überlauf beim Projizieren (endliche, aber riesige Zahlen) wird vor den fachlichen
    // Prüfungen gemeldet, unabhängig von der Reihenfolge der Bezüge.
    for g in geos {
        let (p, q) = match g {
            Geo3::Point(p) | Geo3::Circle { center: p, .. } | Geo3::Sphere { center: p, .. } => {
                (*p, *p)
            }
            Geo3::Axis {
                origin,
                dir,
                t0,
                t1,
                ..
            } => {
                if t1 < t0 {
                    return Err(invalid(Why::AxisRange));
                }
                match axis_ends(*origin, *dir, *t0, *t1) {
                    Ok((_, p0, p1)) => (p0, p1),
                    // Fehlende Richtung meldet die fachliche Prüfung.
                    Err(_) if dir.normalize_or_zero() == DVec3::ZERO => (*origin, *origin),
                    Err(e) => return Err(e),
                }
            }
            Geo3::Line { a, b } => (*a, *b),
        };
        Seg::of(basis, p, q)?;
    }
    let tol = geom_tol(extent);
    let m = match kind {
        DimKind::Horizontal => linear(geos, basis, DVec2::X, tol)?,
        DimKind::Vertical => linear(geos, basis, DVec2::Y, tol)?,
        DimKind::Aligned => aligned(geos, basis, tol)?,
        DimKind::Diameter => round(&geos[0], basis, false)?,
        DimKind::Radius => round(&geos[0], basis, true)?,
        DimKind::Angle => angle(geos, basis, tol)?,
    };
    if m.value.is_finite() && anchor_is_finite(&m.anchor) {
        Ok(m)
    } else {
        Err(invalid(Why::NotFinite))
    }
}

/// Bildlänge, bis zu der eine Kante als Punkt gilt.
fn point_len(tol: f64) -> f64 {
    16.0 * tol
}

/// Querachse zur Messrichtung: `dir` um 90° gedreht, nach rechts bzw. oben gerichtet.
fn cross_axis(dir: DVec2) -> DVec2 {
    rightward(dir.perp())
}

/// Zwei Punkte geordnet nach Querlage `·w` (kleinere zuerst; bei Gleichstand wie gegeben).
fn by_cross(p: DVec2, q: DVec2, w: DVec2) -> (DVec2, DVec2) {
    if q.dot(w) < p.dot(w) { (q, p) } else { (p, q) }
}

/// Lage eines Bezugs entlang `u` und seine Spanne quer dazu.
struct Loc {
    at: f64,
    span: (DVec2, DVec2),
}

impl Loc {
    fn point(q: DVec2, u: DVec2) -> Loc {
        Loc {
            at: q.dot(u),
            span: (q, q),
        }
    }

    /// Bildstrecke `s`, deren Richtung `v` quer zu `u` stehen muss.
    fn across(s: &Seg, v: DVec2, u: DVec2, w: DVec2) -> Result<Loc, DimError> {
        if v.dot(u).abs() > ACROSS_TOL * v.length() {
            return Err(DimError::NotAcross);
        }
        let at = 0.5 * (s.a.dot(u) + s.b.dot(u));
        let end = |p: DVec2| u * at + w * p.dot(w);
        Ok(Loc {
            at,
            span: by_cross(end(s.a), end(s.b), w),
        })
    }
}

/// Lage entlang `u` und Spanne eines Bezugs für ein waagerechtes/senkrechtes Maß.
fn locus(g: &Geo3, basis: &Basis, u: DVec2, w: DVec2, tol: f64) -> Result<Loc, DimError> {
    match g {
        Geo3::Point(p) | Geo3::Circle { center: p, .. } | Geo3::Sphere { center: p, .. } => {
            Ok(Loc::point(basis.project(*p), u))
        }
        Geo3::Line { a, b } => {
            let s = Seg::of(basis, *a, *b)?;
            if s.len <= point_len(tol) {
                return Ok(Loc::point((s.a + s.b) * 0.5, u));
            }
            Loc::across(&s, s.v, u, w)
        }
        Geo3::Axis {
            origin,
            dir,
            t0,
            t1,
            ..
        } => {
            let (e, p0, p1) = axis_ends(*origin, *dir, *t0, *t1)?;
            let s = Seg::of(basis, p0, p1)?;
            if e.dot(unit3(basis.d)?).abs() >= 1.0 - VIEW_TOL {
                return Ok(Loc::point((s.a + s.b) * 0.5, u));
            }
            Loc::across(&s, basis.project(e), u, w)
        }
    }
}

fn unit3(v: DVec3) -> Result<DVec3, DimError> {
    let n = v.normalize_or_zero();
    if n == DVec3::ZERO || !n.is_finite() {
        Err(invalid(Why::NoDirection))
    } else {
        Ok(n)
    }
}

fn linear(geos: &[Geo3], basis: &Basis, u: DVec2, tol: f64) -> Result<Measured, DimError> {
    let w = cross_axis(u);
    let la = locus(&geos[0], basis, u, w, tol)?;
    let lb = locus(&geos[1], basis, u, w, tol)?;
    let value = (lb.at - la.at).abs();
    if value.is_nan() {
        return Err(invalid(Why::NotFinite));
    }
    if value < tol {
        return Err(invalid(Why::ZeroLength));
    }
    Ok(Measured {
        value,
        anchor: Anchor::Linear {
            a: la.span.1,
            b: lb.span.1,
            dir: u,
            a_span: la.span,
            b_span: lb.span,
        },
        prefix: None,
    })
}

/// Bildpunkt eines punktartigen Bezugs für ein schräges Maß (`Point` oder als Punkt gesehene
/// Kante), sonst `None`.
fn aligned_point(g: &Geo3, basis: &Basis, tol: f64) -> Result<Option<DVec2>, DimError> {
    match g {
        Geo3::Point(p) => Ok(Some(basis.project(*p))),
        Geo3::Line { a, b } => {
            let s = Seg::of(basis, *a, *b)?;
            Ok((s.len <= point_len(tol)).then(|| (s.a + s.b) * 0.5))
        }
        _ => Ok(None),
    }
}

fn aligned_measured(
    value: f64,
    a: DVec2,
    b: DVec2,
    dir: DVec2,
    spans: ((DVec2, DVec2), (DVec2, DVec2)),
    tol: f64,
) -> Result<Measured, DimError> {
    if !value.is_finite() {
        return Err(invalid(Why::NotFinite));
    }
    if value < tol {
        return Err(invalid(Why::ZeroLength));
    }
    Ok(Measured {
        value,
        anchor: Anchor::Linear {
            a,
            b,
            dir,
            a_span: spans.0,
            b_span: spans.1,
        },
        prefix: None,
    })
}

fn aligned(geos: &[Geo3], basis: &Basis, tol: f64) -> Result<Measured, DimError> {
    let pa = aligned_point(&geos[0], basis, tol)?;
    let pb = aligned_point(&geos[1], basis, tol)?;
    match (pa, pb, &geos[0], &geos[1]) {
        (Some(a), Some(b), _, _) => {
            let diff = b - a;
            let value = diff.length();
            aligned_measured(value, a, b, diff / value, ((a, a), (b, b)), tol)
        }
        (None, None, Geo3::Line { a: a1, b: b1 }, Geo3::Line { a: a2, b: b2 }) => {
            let s1 = Seg::of(basis, *a1, *b1)?;
            let s2 = Seg::of(basis, *a2, *b2)?;
            let (e, e2) = (s1.v / s1.len, s2.v / s2.len);
            if e.perp_dot(e2).abs() > ACROSS_TOL {
                return Err(invalid(Why::AlignedRefs));
            }
            // Bereiche der Kanten entlang e; Punkt der Geraden 1 bzw. 2 zur Lage k entlang e.
            let range = |s: &Seg| {
                let (x, y) = (s.a.dot(e), s.b.dot(e));
                (x.min(y), x.max(y))
            };
            let on1 = |k: f64| s1.a + e * (k - s1.a.dot(e));
            let on2 = |k: f64| s2.a + e * (k - s2.a.dot(e));
            let ((lo1, hi1), (lo2, hi2)) = (range(&s1), range(&s2));
            let (lo, hi) = (lo1.max(lo2), hi1.min(hi2));
            let (a, b) = if lo <= hi {
                let k = 0.5 * (lo + hi);
                (on1(k), on2(k))
            } else if hi1 < lo2 {
                (on1(hi1), on2(lo2))
            } else {
                (on1(lo1), on2(hi2))
            };
            let n = e.perp();
            let sd = (b - a).dot(n);
            let dir = if sd < 0.0 { -n } else { n };
            let w = cross_axis(dir);
            let spans = (by_cross(s1.a, s1.b, w), by_cross(s2.a, s2.b, w));
            aligned_measured(sd.abs(), a, b, dir, spans, tol)
        }
        _ => Err(invalid(Why::AlignedRefs)),
    }
}

/// Einheitsrichtung nach rechts, bei senkrechter Lage nach oben.
fn rightward(t: DVec2) -> DVec2 {
    if t.x < -1e-9 || (t.x.abs() <= 1e-9 && t.y < 0.0) {
        -t
    } else {
        t
    }
}

/// Ø quer über eine Bildstrecke durch `c` mit der Richtung `t` (Einheitsvektor).
fn across_diameter(c: DVec2, t: DVec2, r: f64) -> Measured {
    let t = rightward(t);
    let (a, b) = (c - t * r, c + t * r);
    Measured {
        value: 2.0 * r,
        anchor: Anchor::Linear {
            a,
            b,
            dir: t,
            a_span: (a, a),
            b_span: (b, b),
        },
        prefix: Some('Ø'),
    }
}

/// Ø quer zu einer Zylinderachse mit den Bildenden `c0`, `c1` und der Bildrichtung `q`
/// (Einheitsvektor): Umrisslinien `c(τ) ± r·t`, `t = q⊥`.
fn across_axis_diameter(c0: DVec2, c1: DVec2, q: DVec2, r: f64) -> Measured {
    let t = rightward(q.perp());
    let w = cross_axis(t);
    let a_span = by_cross(c0 - t * r, c1 - t * r, w);
    let b_span = by_cross(c0 + t * r, c1 + t * r, w);
    Measured {
        value: 2.0 * r,
        anchor: Anchor::Linear {
            a: a_span.1,
            b: b_span.1,
            dir: t,
            a_span,
            b_span,
        },
        prefix: Some('Ø'),
    }
}

fn round_anchor(center: DVec2, r: f64, arc: Option<ArcSpan>, radius_kind: bool) -> Measured {
    Measured {
        value: if radius_kind { r } else { 2.0 * r },
        anchor: Anchor::Round {
            center,
            radius: r,
            arc,
        },
        prefix: Some(if radius_kind { 'R' } else { 'Ø' }),
    }
}

fn round(g: &Geo3, basis: &Basis, radius_kind: bool) -> Result<Measured, DimError> {
    let positive = |r: f64| {
        if r > 0.0 {
            Ok(r)
        } else {
            Err(invalid(Why::NoRadius))
        }
    };
    let d = unit3(basis.d)?;
    match g {
        Geo3::Circle {
            center,
            normal,
            radius,
            arc,
        } => {
            let r = positive(*radius)?;
            let n = unit3(*normal)?;
            let c = basis.project(*center);
            let cos = n.dot(d).abs();
            if cos >= 1.0 - VIEW_TOL {
                // Bogen im Bild aus den Bildern seiner drei Punkte (die Mitte wird im Raum
                // abgezogen: genauer bei großen Koordinaten).
                let span = arc.and_then(|a| {
                    let q = |p: DVec3| basis.project(p - *center);
                    ArcSpan::through(DVec2::ZERO, q(a.start), q(a.mid), q(a.end))
                });
                Ok(round_anchor(c, r, span, radius_kind))
            } else if cos <= VIEW_TOL {
                if radius_kind {
                    return Err(invalid(Why::RadiusSide));
                }
                // Die Kreisebene steht senkrecht zum Bild; ihre Spur verläuft senkrecht zum Bild
                // der Normalen.
                let pn = basis.project(n).normalize_or_zero();
                if pn == DVec2::ZERO {
                    return Err(invalid(Why::NoDirection));
                }
                Ok(across_diameter(c, pn.perp(), r))
            } else {
                Err(invalid(Why::Oblique))
            }
        }
        Geo3::Axis {
            origin,
            dir,
            radius,
            t0,
            t1,
        } => {
            let r = positive(*radius)?;
            let (e, p0, p1) = axis_ends(*origin, *dir, *t0, *t1)?;
            let s = Seg::of(basis, p0, p1)?;
            let cos = e.dot(d).abs();
            if cos > VIEW_TOL && cos < 1.0 - VIEW_TOL {
                Err(invalid(Why::Oblique))
            } else if radius_kind {
                Err(invalid(Why::RadiusAxis))
            } else if cos <= VIEW_TOL {
                let q = basis.project(e).normalize_or_zero();
                if q == DVec2::ZERO {
                    return Err(invalid(Why::NoDirection));
                }
                Ok(across_axis_diameter(s.a, s.b, q, r))
            } else {
                Ok(round_anchor((s.a + s.b) * 0.5, r, None, false))
            }
        }
        Geo3::Sphere { center, radius } => {
            let r = positive(*radius)?;
            Ok(round_anchor(basis.project(*center), r, None, radius_kind))
        }
        Geo3::Point(_) | Geo3::Line { .. } => Err(invalid(Why::RoundRefs)),
    }
}

/// Richtung vom Scheitel `v` weg entlang der Kante `s` (Regel siehe [`measure`]).
fn ray_dir(s: &Seg, v: DVec2) -> DVec2 {
    let e = s.v / s.len;
    let tau = (v - s.a).dot(e) / s.len;
    // Am Anfang oder davor: zur Kante hin (+e). Am Ende oder dahinter: zur Kante hin (−e).
    // Innen: zum näheren Ende (τ < 0,5 → Anfang, −e; sonst Ende, +e).
    let toward_end = tau <= END_PARAM_TOL || (0.5..1.0 - END_PARAM_TOL).contains(&tau);
    if toward_end { e } else { -e }
}

fn angle(geos: &[Geo3], basis: &Basis, tol: f64) -> Result<Measured, DimError> {
    let d = unit3(basis.d)?;
    let seg = |g: &Geo3| match g {
        Geo3::Line { a, b } => {
            let s = Seg::of(basis, *a, *b)?;
            if s.len <= point_len(tol) {
                return Err(invalid(Why::PointEdge));
            }
            if unit3(*b - *a)?.dot(d).abs() > ANGLE_PLANE_TOL {
                return Err(invalid(Why::AngleOblique));
            }
            Ok(s)
        }
        _ => Err(invalid(Why::AngleRefs)),
    };
    let s1 = seg(&geos[0])?;
    let s2 = seg(&geos[1])?;
    let (e1, e2) = (s1.v / s1.len, s2.v / s2.len);
    let sin = e1.perp_dot(e2);
    if !sin.is_finite() {
        return Err(invalid(Why::NotFinite));
    }
    if sin.abs() <= PARALLEL_SIN {
        return Err(invalid(Why::Parallel));
    }
    // s1.a + t·e1 = s2.a + r·e2  ⇒  t = ((s2.a − s1.a) × e2) / (e1 × e2).
    let t = (s2.a - s1.a).perp_dot(e2) / sin;
    let vertex = s1.a + e1 * t;
    if !vertex.is_finite() {
        return Err(invalid(Why::NotFinite));
    }
    let d1 = ray_dir(&s1, vertex);
    let d2 = ray_dir(&s2, vertex);
    let value = d1.perp_dot(d2).abs().atan2(d1.dot(d2)).to_degrees();
    Ok(Measured {
        value,
        anchor: Anchor::Angle { vertex, d1, d2 },
        prefix: None,
    })
}

/// Mindest- und Höchstzahl der Bezüge einer Maßart (bei Ketten je Kettenglied).
pub fn ref_count(kind: DimKind) -> (usize, usize) {
    match kind {
        DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned | DimKind::Angle => (2, 2),
        DimKind::Diameter | DimKind::Radius => (1, 1),
    }
}

impl DimKind {
    /// Siehe [`ref_count`].
    pub fn ref_count(self) -> (usize, usize) {
        ref_count(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_i18n::{english, german, with_lang};

    const ALL_WHY: [Why; 15] = [
        Why::ZeroLength,
        Why::Oblique,
        Why::NotFinite,
        Why::WrongCount,
        Why::AlignedRefs,
        Why::RoundRefs,
        Why::AngleRefs,
        Why::NoRadius,
        Why::NoDirection,
        Why::PointEdge,
        Why::Parallel,
        Why::AxisRange,
        Why::RadiusSide,
        Why::RadiusAxis,
        Why::AngleOblique,
    ];

    #[test]
    fn invalid_reasons_speak_english() {
        for why in ALL_WHY {
            let en = with_lang(&english(), || invalid(why).to_string());
            let de = with_lang(&german(), || invalid(why).to_string());
            assert!(en.starts_with("The dimension is not valid: "), "{en}");
            assert!(de.starts_with("Das Maß ist ungültig: "), "{de}");
            assert!(!en.chars().any(|c| "äöüßÄÖÜ„".contains(c)), "{en}");
            assert_ne!(en, de);
        }
        assert_eq!(
            with_lang(&german(), || invalid(Why::ZeroLength).to_string()),
            "Das Maß ist ungültig: Das Maß hat die Länge null."
        );
    }
}
