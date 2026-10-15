//! Toleranzen und Arbeitsgrenzen. Toleranzen skalieren mit der Ausdehnung `E`
//! (größte Kante der Hüllbox aller Körper, mindestens 1e-9); feste absolute Werte gibt es nicht.

/// Faktor für `SELF_EPS = SELF_EPS_REL · E` (Selbsttreffer zählen nie als Verdeckung).
pub const SELF_EPS_REL: f64 = 1e-7;
/// Faktor für `OWN_EPS = OWN_EPS_REL · E` (Treffer auf den eigenen Flächen eines Strichs).
pub const OWN_EPS_REL: f64 = 1e-5;
/// Genauigkeit der Umrisskurven (Torus, Freiform) in mm.
pub const SILHOUETTE_TOL: f64 = 1e-4;
/// Winkel, unterhalb dessen eine Kante als glatt gilt (0,5°) in Radiant.
pub const SMOOTH_ANGLE: f64 = 0.5 * std::f64::consts::PI / 180.0;
/// Abbruchgenauigkeit der Bisektion, relativ zur Parameterbreite.
pub const BISECT_TOL: f64 = 1e-9;

/// Höchstzahl an Strichen (nach dem Verwerfen der in der Ansicht zu Punkten entarteten).
pub const MAX_STROKES: u64 = 20_000;
/// Speichergrenze beim Sammeln der Kanten- und Umrissstriche vor dem Verwerfen entarteter
/// Striche (`4 · MAX_STROKES`).
pub const MAX_RAW_STROKES: u64 = 4 * MAX_STROKES;
/// Höchstzahl an Teilstücken.
pub const MAX_PIECES: u64 = 200_000;
/// Höchstzahl an Rechenpunkten (Raster und dichte Umrisspunkte) je Torus- oder Freiformfläche.
pub const MAX_OUTLINE_POINTS: u64 = 400_000;
/// Höchstzahl an Strichpaar-Tests.
pub const MAX_PAIR_TESTS: u64 = 50_000_000;
/// Höchstzahl an Sichtprüfungen (Strahltests).
pub const MAX_RAY_TESTS: u64 = 20_000_000;
/// Höchste Teilungstiefe.
pub const MAX_SUBDIVISION_DEPTH: u32 = 40;

/// Geometrische Toleranz für die Ausdehnung `extent`: `max(1e-9, 64·ε·E)`.
pub fn geom_tol(extent: f64) -> f64 {
    (64.0 * f64::EPSILON * extent).max(1e-9)
}

/// Selbsttreffer-Schwelle für die Ausdehnung `extent`.
pub fn self_eps(extent: f64) -> f64 {
    SELF_EPS_REL * extent.max(1e-9)
}

/// Schwelle für Treffer auf den eigenen Flächen eines Strichs.
pub fn own_eps(extent: f64) -> f64 {
    OWN_EPS_REL * extent.max(1e-9)
}
