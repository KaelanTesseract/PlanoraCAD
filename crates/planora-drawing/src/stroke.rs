//! Striche: Kanten der Körper als Kurvenstücke mit Herkunft. Nähte und glatte Kanten
//! (Tangentenübergänge) werden nicht zu Strichen.

use crate::DrawBody;
use crate::basis::Basis;
use crate::error::DrawError;
use crate::limits::{MAX_RAW_STROKES, SMOOTH_ANGLE, geom_tol};
use planora_i18n::tr;
use planora_kernel::{Curve, Edge, Face, Id, Solid, Surface};
use planora_math::{DVec2, DVec3};

/// Zahl der Stützstellen, an denen die Flächennormalen einer Kante verglichen werden.
const SMOOTH_SAMPLES: usize = 5;
/// Probepunkte je Kante für die Ausdehnung.
const EXTENT_SAMPLES: usize = 33;
/// Probepunkte für Hüllbox und Länge eines Strichs.
const BBOX_SAMPLES: usize = 65;

/// Woher ein Strich stammt.
#[derive(Clone, Debug, PartialEq)]
pub enum StrokeOrigin {
    /// Kante eines Körpers: Körperindex im Eingabefeld und Kantenkennung (Index in `solid.edges`).
    Edge { body: usize, edge: usize },
    /// Umriss einer Fläche (Körperindex, Flächenindex).
    Silhouette { body: usize, face: usize },
}

/// Kurvenstück `curve(t)` für `t` zwischen `t0` und `t1`.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
    pub origin: StrokeOrigin,
}

impl Stroke {
    /// Raumpunkt bei Parameter `t`.
    pub fn point(&self, t: f64) -> DVec3 {
        self.curve.point(t)
    }

    /// Auf die Bildebene projizierter Punkt.
    pub fn point2(&self, basis: &Basis, t: f64) -> DVec2 {
        basis.project(self.curve.point(t))
    }

    /// Ableitung des projizierten Punkts nach `t` (analytisch über die Kurventangente).
    pub fn deriv2(&self, basis: &Basis, t: f64) -> DVec2 {
        let d = self.curve.tangent(t);
        DVec2::new(d.dot(basis.ex), d.dot(basis.ey))
    }

    /// Länge des Parameterbereichs (positiv).
    pub fn param_len(&self) -> f64 {
        (self.t1 - self.t0).abs()
    }

    fn sample_t(&self, i: usize, n: usize) -> f64 {
        self.t0 + (self.t1 - self.t0) * i as f64 / n as f64
    }

    /// Hüllbox der Projektion: dichte Probe, bei Freiformkurven dazu die projizierten
    /// Kontrollpunkte; der Rand wird um `pad` erweitert (üblich `geom_tol(extent)`).
    pub fn bbox2(&self, basis: &Basis, pad: f64) -> (DVec2, DVec2) {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        let mut add = |p: DVec2| {
            lo = lo.min(p);
            hi = hi.max(p);
        };
        for i in 0..=BBOX_SAMPLES - 1 {
            add(self.point2(basis, self.sample_t(i, BBOX_SAMPLES - 1)));
        }
        // Kreis und Ellipse: exakte Achsextrema der Projektion (f'(t) = 0 bei t = atan2(B, A) + k·π).
        let conic = match &self.curve {
            Curve::Circle { frame, radius } => Some((frame.x * *radius, frame.y * *radius)),
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => Some((frame.x * *major, frame.y * *minor)),
            _ => None,
        };
        if let Some((ax, ay)) = conic {
            let (lo_t, hi_t) = (self.t0.min(self.t1), self.t0.max(self.t1));
            for w in [basis.ex, basis.ey] {
                let t_star = ay.dot(w).atan2(ax.dot(w));
                let k0 = ((lo_t - t_star) / std::f64::consts::PI).ceil() as i64;
                let k1 = ((hi_t - t_star) / std::f64::consts::PI).floor() as i64;
                for k in k0..=k1 {
                    add(self.point2(basis, t_star + k as f64 * std::f64::consts::PI));
                }
            }
        }
        if let Curve::Nurbs(c) = &self.curve {
            for &p in c.control_points() {
                add(basis.project(p));
            }
        }
        (lo - DVec2::splat(pad), hi + DVec2::splat(pad))
    }

    /// Ist die projizierte Länge kleiner als `tol` (Kante parallel zur Blickrichtung)?
    pub fn is_degenerate_2d(&self, basis: &Basis, tol: f64) -> bool {
        let mut len = 0.0;
        let mut prev = self.point2(basis, self.t0);
        for i in 1..BBOX_SAMPLES {
            let p = self.point2(basis, self.sample_t(i, BBOX_SAMPLES - 1));
            len += p.distance(prev);
            prev = p;
        }
        len < tol
    }
}

/// Größte Kante der Hüllbox aller Eckpunkte und Kantenpunkte, mindestens `1e-9`.
pub fn extent_of(bodies: &[DrawBody]) -> f64 {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for b in bodies {
        let s = b.solid;
        for (_, v) in s.vertices.iter() {
            lo = lo.min(v.point);
            hi = hi.max(v.point);
        }
        for (_, e) in s.edges.iter() {
            for i in 0..EXTENT_SAMPLES {
                let t = e.t0 + (e.t1 - e.t0) * i as f64 / (EXTENT_SAMPLES - 1) as f64;
                let p = e.curve.point(t);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return 1e-9;
    }
    let d = hi - lo;
    d.x.max(d.y).max(d.z).max(1e-9)
}

/// Flächen, an denen die Halbkanten einer Kante liegen (in Reihenfolge des Auftretens).
pub(crate) fn edge_faces(solid: &Solid) -> Vec<Vec<usize>> {
    let mut uses: Vec<Vec<usize>> = vec![Vec::new(); solid.edges.len()];
    for (fid, f) in solid.faces.iter() {
        for lp in std::iter::once(&f.outer).chain(f.inner.iter()) {
            for he in &lp.half_edges {
                if let Some(u) = uses.get_mut(he.edge.index()) {
                    u.push(fid.index());
                }
            }
        }
    }
    uses
}

/// Flächen je Kante eines Körpers (einmal je Körper aufgebaut): Kantenindex → Indizes der
/// anliegenden Flächen in `solid.faces` (ohne Wiederholung).
#[derive(Clone, Debug)]
pub struct EdgeFaces {
    faces: Vec<Vec<usize>>,
}

impl EdgeFaces {
    pub fn new(solid: &Solid) -> EdgeFaces {
        let mut faces = edge_faces(solid);
        for f in &mut faces {
            f.sort_unstable();
            f.dedup();
        }
        EdgeFaces { faces }
    }

    /// Eigene Flächen eines Strichs: bei einer Kante die anliegenden Flächen, bei einem Umriss
    /// seine Fläche.
    pub fn own_faces(&self, stroke: &Stroke) -> Vec<usize> {
        match stroke.origin {
            StrokeOrigin::Edge { edge, .. } => self.faces.get(edge).cloned().unwrap_or_default(),
            StrokeOrigin::Silhouette { face, .. } => vec![face],
        }
    }
}

/// Außennormale der Fläche am Raumpunkt `p`; `hint` führt die Suche bei Freiformflächen.
fn outward_normal(face: &Face, p: DVec3, hint: &mut Option<(f64, f64)>) -> DVec3 {
    let (u, v) = match (&face.surface, *hint) {
        (Surface::Nurbs(_), Some(h)) => face.surface.project_near(p, h),
        (s, _) => s.project(p),
    };
    *hint = Some((u, v));
    let n = face.surface.normal(u, v);
    if face.same_sense { n } else { -n }
}

/// Ist die Kante glatt: Winkel der Normalen beider Flächen an allen Stützstellen höchstens
/// `SMOOTH_ANGLE`?
fn is_smooth(solid: &Solid, edge: &Edge, fa: usize, fb: usize) -> bool {
    let a = &solid.faces[Id::<Face>::from_index(fa)];
    let b = &solid.faces[Id::<Face>::from_index(fb)];
    let (mut ha, mut hb) = (None, None);
    for i in 0..SMOOTH_SAMPLES {
        let t = edge.t0 + (edge.t1 - edge.t0) * i as f64 / (SMOOTH_SAMPLES - 1) as f64;
        let p = edge.curve.point(t);
        let na = outward_normal(a, p, &mut ha);
        let nb = outward_normal(b, p, &mut hb);
        if !(na.is_finite() && nb.is_finite()) || na.length() < 0.5 || nb.length() < 0.5 {
            return false;
        }
        let angle = na.cross(nb).length().atan2(na.dot(nb));
        if angle > SMOOTH_ANGLE {
            return false;
        }
    }
    true
}

/// Je Kante der Körper ein Strich, außer Nähten (beide Seiten derselben Fläche) und glatten
/// Kanten. Striche, die in der Ansicht zu einem Punkt entarten, bleiben erhalten.
pub fn edge_strokes(
    bodies: &[DrawBody],
    _basis: &Basis,
    _extent: f64,
) -> Result<Vec<Stroke>, DrawError> {
    let mut out = Vec::new();
    for (bi, body) in bodies.iter().enumerate() {
        let solid = body.solid;
        let uses = edge_faces(solid);
        for (eid, e) in solid.edges.iter() {
            let ei = eid.index();
            if !(e.t0.is_finite() && e.t1.is_finite()) {
                return Err(DrawError::NonFinite);
            }
            let faces = &uses[ei];
            if faces.len() < 2 {
                return Err(DrawError::InvalidBody(tr!("drawing-err-edge-faces")));
            }
            // Naht: alle Halbkanten in derselben Fläche.
            if faces.iter().all(|&f| f == faces[0]) {
                continue;
            }
            if faces.len() == 2 && is_smooth(solid, e, faces[0], faces[1]) {
                continue;
            }
            out.push(Stroke {
                curve: e.curve.clone(),
                t0: e.t0,
                t1: e.t1,
                origin: StrokeOrigin::Edge { body: bi, edge: ei },
            });
            if out.len() as u64 > MAX_RAW_STROKES {
                return Err(DrawError::TooComplex(out.len() as u64));
            }
        }
    }
    Ok(out)
}

/// Rand für `bbox2` zur Ausdehnung `extent`.
pub fn bbox_pad(extent: f64) -> f64 {
    geom_tol(extent)
}
