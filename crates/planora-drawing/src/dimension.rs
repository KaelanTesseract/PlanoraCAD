//! Datentypen für Maße (Teilprojekt 12): Verweise auf Modellgeometrie, Maßarten, Toleranzen,
//! Maßketten und die Fehler einer Maßangabe; die Messung ([`measure`]) aus aufgelöster
//! Modellgeometrie ([`Geo3`]) und der Ansichtsbasis.

use crate::basis::Basis;
use crate::limits::geom_tol;
use crate::model::{ViewId, ViewKind};
use planora_math::{DVec2, DVec3};

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
    Limits { upper: f64, lower: f64 },
    LimitSizes { upper: f64, lower: f64 },
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

/// Aufgelöste Modellgeometrie eines Bezugs.
#[derive(Clone, Debug, PartialEq)]
pub enum Geo3 {
    Point(DVec3),
    Line {
        a: DVec3,
        b: DVec3,
    },
    Circle {
        center: DVec3,
        normal: DVec3,
        radius: f64,
    },
    Axis {
        origin: DVec3,
        dir: DVec3,
        radius: f64,
    },
    Sphere {
        center: DVec3,
        radius: f64,
    },
}

/// Fehler einer Maßangabe.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum DimError {
    #[error("Die Kante steht nicht quer zum Maß.")]
    NotAcross,
    #[error("Der Bezug ist in dieser Ansicht nicht zu sehen.")]
    NotVisible,
    #[error("Der Bezug ist nicht eindeutig.")]
    Ambiguous,
    #[error("Der Bezug wurde nicht gefunden: {0}")]
    Lost(String),
    #[error("In der Isometrie gibt es keine Maße.")]
    NotAllowedInIsometric,
    #[error("Es gibt zu viele Maße in dieser Zeichnung.")]
    TooMany,
    #[error("Das Maß ist ungültig: {0}")]
    Invalid(String),
}

/// Ergebnis einer Messung in **Ansichtskoordinaten** (ungeskaliert, mm; y nach oben).
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// Zwei Messpunkte und die Messrichtung (Einheitsvektor): Horizontal/Vertical/Aligned und
    /// Ø quer. Der Wert ist `|(b − a)·dir|`.
    Linear { a: DVec2, b: DVec2, dir: DVec2 },
    /// Kreis mit Blick in Achsrichtung (oder Kugel): Mittelpunkt und Radius.
    Round { center: DVec2, radius: f64 },
    /// Winkel zwischen zwei Geraden: Scheitel und zwei Einheitsrichtungen vom Scheitel weg.
    Angle { vertex: DVec2, d1: DVec2, d2: DVec2 },
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

const ZERO_LENGTH: &str = "Das Maß hat die Länge null.";
const OBLIQUE: &str = "Der Kreis erscheint schräg.";
const NOT_FINITE: &str = "Der Bezug enthält Zahlen, mit denen sich nicht rechnen lässt.";
const WRONG_COUNT: &str = "Das Maß hat die falsche Zahl von Bezügen.";
const ALIGNED_REFS: &str = "Ein schräges Maß braucht zwei Punkte oder zwei parallele Kanten.";
const ROUND_REFS: &str =
    "Durchmesser und Radius brauchen einen Kreis, einen Zylinder oder eine Kugel.";
const ANGLE_REFS: &str = "Ein Winkelmaß braucht zwei Kanten.";
const NO_RADIUS: &str = "Der Kreis hat keinen Radius.";
const NO_DIRECTION: &str = "Der Bezug hat keine Richtung.";
const POINT_EDGE: &str = "Die Kante erscheint in dieser Ansicht als Punkt.";
const PARALLEL: &str = "Die Kanten sind parallel und bilden keinen Winkel.";

fn invalid(text: &str) -> DimError {
    DimError::Invalid(text.into())
}

fn geo_is_finite(g: &Geo3) -> bool {
    match g {
        Geo3::Point(p) => p.is_finite(),
        Geo3::Line { a, b } => a.is_finite() && b.is_finite(),
        Geo3::Circle {
            center,
            normal,
            radius,
        } => center.is_finite() && normal.is_finite() && radius.is_finite(),
        Geo3::Axis {
            origin,
            dir,
            radius,
        } => origin.is_finite() && dir.is_finite() && radius.is_finite(),
        Geo3::Sphere { center, radius } => center.is_finite() && radius.is_finite(),
    }
}

fn anchor_is_finite(a: &Anchor) -> bool {
    match a {
        Anchor::Linear { a, b, dir } => a.is_finite() && b.is_finite() && dir.is_finite(),
        Anchor::Round { center, radius } => center.is_finite() && radius.is_finite(),
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
            Err(invalid(NOT_FINITE))
        }
    }
}

/// Misst ein Maß der Art `kind` an den aufgelösten Bezügen `geos` in der Ansicht mit der Basis
/// `basis` (wie von [`Basis::of`]: orthonormal, `ex` rechts, `ey` oben). `extent` ist die
/// Ausdehnung des Modells (größte Kante der Hüllbox); daraus `tol = geom_tol(extent)`.
/// Der Wert hängt nur von der Basis ab, nicht vom Blattmaßstab (Modell-mm bzw. Grad).
///
/// Regeln:
/// - `view_kind == Isometric` → [`DimError::NotAllowedInIsometric`] (vor allen anderen Prüfungen).
/// - Falsche Bezugszahl ([`ref_count`]), nicht endliche Zahlen in Bezügen, Basis oder `extent`
///   (oder `extent < 0`), Überlauf beim Rechnen → `Invalid`. Nie ein Absturz.
/// - **Horizontal/Vertical:** Messrichtung `u = (1, 0)` (entlang `ex`) bzw. `(0, 1)` (entlang
///   `ey`), Querachse `w` die andere Bildachse. Lage je Bezug entlang `u`: `Point`, `Circle`
///   (Mitte), `Sphere` (Mitte) → Bildpunkt; `Line` mit Bildlänge `≤ 16·tol` (Kante entlang der
///   Blickrichtung, wie in `compute_view` verworfen) → Bildpunkt (Mitte); sonst muss die
///   Bildrichtung `v` quer stehen (`|v·u| ≤ ACROSS_TOL·|v|`, sonst `NotAcross`), Lage =
///   Mittel der beiden Enden, Ankerpunkt = diese Lage mit der **größeren** Querlage der beiden
///   Enden (bei Waagerecht das obere, bei Senkrecht das rechte Ende); `Axis` mit
///   `|dir·d| ≥ 1 − VIEW_TOL` → Bildpunkt des Ursprungs, sonst muss die Bildrichtung quer
///   stehen, Ankerpunkt = Bild des Ursprungs. Wert `|lage_b − lage_a|`; `< tol` →
///   `Invalid("Das Maß hat die Länge null.")`. Anker `Linear { a, b, dir: u }` (a vom ersten,
///   b vom zweiten Bezug).
/// - **Aligned:** zwei Punkte (`Point` oder als Punkt gesehene `Line`): `dir = (b − a)/|b − a|`;
///   oder zwei Kanten, deren Bildrichtungen parallel sind (Sinus `≤ ACROSS_TOL`): Abstand der
///   Geraden im Bild, `a` = Bildmitte der ersten Kante, `b` = Fußpunkt auf der zweiten Geraden,
///   `dir = (b − a)/|b − a|`. Alles andere → `Invalid`.
/// - **Diameter/Radius:** `Circle` mit `|n·d| ≥ 1 − VIEW_TOL` → `Round` (Mitte im Bild, `r`),
///   Wert `2r`/`r`, Vorsatz `'Ø'`/`'R'`; mit `|n·d| ≤ VIEW_TOL` (von der Seite) → Ø als
///   `Linear` quer über die Bildstrecke (`a = c − r·t`, `b = c + r·t`, `t` Einheitsrichtung der
///   Strecke, Radius → `NotAcross`); `Axis` mit `|dir·d| ≤ VIEW_TOL` → Ø als `Linear` quer zur
///   Bildachse um das Bild des Ursprungs (Radius → `NotAcross`); `Axis` mit
///   `|dir·d| ≥ 1 − VIEW_TOL` → Ø als `Round` (Radius → `NotAcross`); `Sphere` → `Round`.
///   Sonst `Invalid("Der Kreis erscheint schräg.")`. Die Richtung `t` eines linearen Ø zeigt
///   nach rechts, bei senkrechter Lage (`|t.x| ≤ 1e-9`) nach oben.
/// - **Angle:** zwei `Line` mit Bildlänge `> 16·tol`, Sinus des Winkels der Bildgeraden
///   `> PARALLEL_SIN` (sonst `Invalid`). Scheitel = Schnitt der unbegrenzten Bildgeraden.
///   Richtung je Kante: Liegt der Scheitel (Kantenparameter `τ`, 0 am Anfang, 1 am Ende) bis auf
///   `END_PARAM_TOL` an einem Ende oder außerhalb, zeigt sie vom Scheitel zur Kante hin; liegt
///   er innerhalb, zeigt sie zum näheren Ende (`τ < 0,5` → Anfang, sonst Ende). Wert =
///   `atan2(|d1 × d2|, d1·d2)` in Grad, im offenen Bereich (0°, 180°).
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
        return Err(invalid(WRONG_COUNT));
    }
    let basis_ok = basis.d.is_finite() && basis.ex.is_finite() && basis.ey.is_finite();
    if !(basis_ok && extent.is_finite() && extent >= 0.0 && geos.iter().all(geo_is_finite)) {
        return Err(invalid(NOT_FINITE));
    }
    // Überlauf beim Projizieren (endliche, aber riesige Zahlen) wird vor den fachlichen
    // Prüfungen gemeldet, unabhängig von der Reihenfolge der Bezüge.
    for g in geos {
        let (p, q) = match g {
            Geo3::Point(p)
            | Geo3::Circle { center: p, .. }
            | Geo3::Sphere { center: p, .. }
            | Geo3::Axis { origin: p, .. } => (*p, *p),
            Geo3::Line { a, b } => (*a, *b),
        };
        Seg::of(basis, p, q)?;
    }
    let tol = geom_tol(extent);
    let m = match kind {
        DimKind::Horizontal => linear(geos, basis, DVec2::X, DVec2::Y, tol)?,
        DimKind::Vertical => linear(geos, basis, DVec2::Y, DVec2::X, tol)?,
        DimKind::Aligned => aligned(geos, basis, tol)?,
        DimKind::Diameter => round(&geos[0], basis, false)?,
        DimKind::Radius => round(&geos[0], basis, true)?,
        DimKind::Angle => angle(geos, basis, tol)?,
    };
    if m.value.is_finite() && anchor_is_finite(&m.anchor) {
        Ok(m)
    } else {
        Err(invalid(NOT_FINITE))
    }
}

/// Bildlänge, bis zu der eine Kante als Punkt gilt.
fn point_len(tol: f64) -> f64 {
    16.0 * tol
}

/// Lage entlang `u` und Ankerpunkt eines Bezugs für ein waagerechtes/senkrechtes Maß.
fn locus(g: &Geo3, basis: &Basis, u: DVec2, w: DVec2, tol: f64) -> Result<(f64, DVec2), DimError> {
    let point = |p: DVec3| {
        let q = basis.project(p);
        (q.dot(u), q)
    };
    match g {
        Geo3::Point(p) | Geo3::Circle { center: p, .. } | Geo3::Sphere { center: p, .. } => {
            Ok(point(*p))
        }
        Geo3::Line { a, b } => {
            let s = Seg::of(basis, *a, *b)?;
            if s.len <= point_len(tol) {
                let q = (s.a + s.b) * 0.5;
                return Ok((q.dot(u), q));
            }
            if s.v.dot(u).abs() > ACROSS_TOL * s.len {
                return Err(DimError::NotAcross);
            }
            let at = 0.5 * (s.a.dot(u) + s.b.dot(u));
            let across = s.a.dot(w).max(s.b.dot(w));
            Ok((at, u * at + w * across))
        }
        Geo3::Axis { origin, dir, .. } => {
            let dn = unit3(*dir)?;
            let along_view = dn.dot(basis.d).abs() >= 1.0 - VIEW_TOL;
            if !along_view {
                let v = basis.project(dn);
                if v.dot(u).abs() > ACROSS_TOL * v.length() {
                    return Err(DimError::NotAcross);
                }
            }
            Ok(point(*origin))
        }
    }
}

fn unit3(v: DVec3) -> Result<DVec3, DimError> {
    let n = v.normalize_or_zero();
    if n == DVec3::ZERO || !n.is_finite() {
        Err(invalid(NO_DIRECTION))
    } else {
        Ok(n)
    }
}

fn linear(
    geos: &[Geo3],
    basis: &Basis,
    u: DVec2,
    w: DVec2,
    tol: f64,
) -> Result<Measured, DimError> {
    let (sa, a) = locus(&geos[0], basis, u, w, tol)?;
    let (sb, b) = locus(&geos[1], basis, u, w, tol)?;
    let value = (sb - sa).abs();
    if value.is_nan() {
        return Err(invalid(NOT_FINITE));
    }
    if value < tol {
        return Err(invalid(ZERO_LENGTH));
    }
    Ok(Measured {
        value,
        anchor: Anchor::Linear { a, b, dir: u },
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

fn aligned(geos: &[Geo3], basis: &Basis, tol: f64) -> Result<Measured, DimError> {
    let pa = aligned_point(&geos[0], basis, tol)?;
    let pb = aligned_point(&geos[1], basis, tol)?;
    let (a, b) = match (pa, pb, &geos[0], &geos[1]) {
        (Some(a), Some(b), _, _) => (a, b),
        (None, None, Geo3::Line { a: a1, b: b1 }, Geo3::Line { a: a2, b: b2 }) => {
            let s1 = Seg::of(basis, *a1, *b1)?;
            let s2 = Seg::of(basis, *a2, *b2)?;
            let (v1, v2) = (s1.v / s1.len, s2.v / s2.len);
            if v1.perp_dot(v2).abs() > ACROSS_TOL {
                return Err(invalid(ALIGNED_REFS));
            }
            // Normale der ersten Geraden; Fußpunkt der Senkrechten von der Mitte der ersten
            // Kante auf die zweite Gerade (Lage der zweiten Geraden aus ihrer Mitte).
            let n = v1.perp();
            let a = (s1.a + s1.b) * 0.5;
            let m2 = (s2.a + s2.b) * 0.5;
            let sd = (m2 - a).dot(n);
            (a, a + n * sd)
        }
        _ => return Err(invalid(ALIGNED_REFS)),
    };
    let diff = b - a;
    let value = diff.length();
    if !value.is_finite() {
        return Err(invalid(NOT_FINITE));
    }
    if value < tol {
        return Err(invalid(ZERO_LENGTH));
    }
    Ok(Measured {
        value,
        anchor: Anchor::Linear {
            a,
            b,
            dir: diff / value,
        },
        prefix: None,
    })
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
    Measured {
        value: 2.0 * r,
        anchor: Anchor::Linear {
            a: c - t * r,
            b: c + t * r,
            dir: t,
        },
        prefix: Some('Ø'),
    }
}

fn round_anchor(center: DVec2, r: f64, radius_kind: bool) -> Measured {
    Measured {
        value: if radius_kind { r } else { 2.0 * r },
        anchor: Anchor::Round { center, radius: r },
        prefix: Some(if radius_kind { 'R' } else { 'Ø' }),
    }
}

fn round(g: &Geo3, basis: &Basis, radius_kind: bool) -> Result<Measured, DimError> {
    let positive = |r: f64| {
        if r > 0.0 {
            Ok(r)
        } else {
            Err(invalid(NO_RADIUS))
        }
    };
    let d = unit3(basis.d)?;
    match g {
        Geo3::Circle {
            center,
            normal,
            radius,
        } => {
            let r = positive(*radius)?;
            let n = unit3(*normal)?;
            let c = basis.project(*center);
            let cos = n.dot(d).abs();
            if cos >= 1.0 - VIEW_TOL {
                Ok(round_anchor(c, r, radius_kind))
            } else if cos <= VIEW_TOL {
                if radius_kind {
                    return Err(DimError::NotAcross);
                }
                // Die Kreisebene steht senkrecht zum Bild; ihre Spur verläuft senkrecht zum Bild
                // der Normalen.
                let pn = basis.project(n).normalize_or_zero();
                if pn == DVec2::ZERO {
                    return Err(invalid(NO_DIRECTION));
                }
                Ok(across_diameter(c, pn.perp(), r))
            } else {
                Err(invalid(OBLIQUE))
            }
        }
        Geo3::Axis {
            origin,
            dir,
            radius,
        } => {
            let r = positive(*radius)?;
            let a = unit3(*dir)?;
            let c = basis.project(*origin);
            let cos = a.dot(d).abs();
            if radius_kind && cos <= VIEW_TOL {
                Err(DimError::NotAcross)
            } else if cos <= VIEW_TOL {
                let pa = basis.project(a).normalize_or_zero();
                if pa == DVec2::ZERO {
                    return Err(invalid(NO_DIRECTION));
                }
                Ok(across_diameter(c, pa.perp(), r))
            } else if cos >= 1.0 - VIEW_TOL {
                if radius_kind {
                    Err(DimError::NotAcross)
                } else {
                    Ok(round_anchor(c, r, false))
                }
            } else {
                Err(invalid(OBLIQUE))
            }
        }
        Geo3::Sphere { center, radius } => {
            let r = positive(*radius)?;
            Ok(round_anchor(basis.project(*center), r, radius_kind))
        }
        Geo3::Point(_) | Geo3::Line { .. } => Err(invalid(ROUND_REFS)),
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
    let seg = |g: &Geo3| match g {
        Geo3::Line { a, b } => {
            let s = Seg::of(basis, *a, *b)?;
            if s.len <= point_len(tol) {
                Err(invalid(POINT_EDGE))
            } else {
                Ok(s)
            }
        }
        _ => Err(invalid(ANGLE_REFS)),
    };
    let s1 = seg(&geos[0])?;
    let s2 = seg(&geos[1])?;
    let (e1, e2) = (s1.v / s1.len, s2.v / s2.len);
    let sin = e1.perp_dot(e2);
    if !sin.is_finite() {
        return Err(invalid(NOT_FINITE));
    }
    if sin.abs() <= PARALLEL_SIN {
        return Err(invalid(PARALLEL));
    }
    // s1.a + t·e1 = s2.a + r·e2  ⇒  t = ((s2.a − s1.a) × e2) / (e1 × e2).
    let t = (s2.a - s1.a).perp_dot(e2) / sin;
    let vertex = s1.a + e1 * t;
    if !vertex.is_finite() {
        return Err(invalid(NOT_FINITE));
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
