//! Eigener Netz-Prüfer: Sichtbarkeit und Merkmalskanten aus dem Dreiecksnetz von
//! `planora_mesh::tessellate`, ganz ohne Code aus `planora-drawing` (außer der Ansichtsbasis, die
//! nur die Blickrichtung festlegt). Strahl gegen Dreieck nach Möller–Trumbore, eigene
//! Kippabstimmung, eigenes Verschweißen der Netzpunkte.

use planora_drawing::basis::Basis;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3};
use planora_mesh::{Mesh, TessellationOptions, tessellate};
use std::collections::HashMap;

/// Kippwinkel der Referenzstrahlen (rad), unabhängig von dem der Bibliothek.
pub const TILT_REF: f64 = 1e-4;
/// Zahl der Kipprichtungen (45°-Schritte).
pub const TILT_REF_DIRECTIONS: usize = 8;
/// Mindestwinkel zwischen den Normalen zweier Dreiecke, ab dem ihre gemeinsame Kante eine
/// Merkmalskante ist.
pub const FEATURE_ANGLE_DEG: f64 = 20.0;
/// Rastergröße für die Vorauswahl der Dreiecke (je Bildachse).
const GRID: usize = 128;
/// `|n · d|` darunter: Dreieck von der Seite gesehen (weder Vorder- noch Rückseite).
const EDGE_ON: f64 = 1e-9;

/// Sehnentoleranz des Referenznetzes zur Ausdehnung `extent`.
pub fn chord_tol(extent: f64) -> f64 {
    1e-3 * extent
}

/// Größte Kante der Hüllbox aller Netzpunkte (eigene Ausdehnung).
pub fn mesh_extent(meshes: &[Mesh]) -> f64 {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for m in meshes {
        for &p in &m.positions {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let d = hi - lo;
    d.x.max(d.y).max(d.z).max(1e-9)
}

/// Feines Netz des Körpers: Sehnentoleranz `1e-3 · extent`.
pub fn fine_mesh(solid: &Solid, extent: f64) -> Mesh {
    mesh_with_chord(solid, chord_tol(extent))
}

/// Netz des Körpers mit der Sehnentoleranz `chord`.
pub fn mesh_with_chord(solid: &Solid, chord: f64) -> Mesh {
    tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .expect("Referenznetz")
}

fn triangles(mesh: &Mesh) -> impl Iterator<Item = [DVec3; 3]> + '_ {
    let (tris, _) = mesh.indices.as_chunks::<3>();
    tris.iter().map(|t| {
        [
            mesh.positions[t[0] as usize],
            mesh.positions[t[1] as usize],
            mesh.positions[t[2] as usize],
        ]
    })
}

/// Möller–Trumbore: Abstand `s` des Schnitts von `o + s·d` mit dem Dreieck (Ränder
/// eingeschlossen), `None` ohne Schnitt oder bei parallelem Strahl.
fn ray_triangle(o: DVec3, d: DVec3, t: &[DVec3; 3]) -> Option<f64> {
    const EPS_DET: f64 = 1e-14;
    const EPS_BARY: f64 = 1e-12;
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let pv = d.cross(e2);
    let det = e1.dot(pv);
    let scale = e1.length() * e2.length();
    if det.abs() <= EPS_DET * scale {
        return None;
    }
    let inv = 1.0 / det;
    let tv = o - t[0];
    let u = tv.dot(pv) * inv;
    if !(-EPS_BARY..=1.0 + EPS_BARY).contains(&u) {
        return None;
    }
    let qv = tv.cross(e1);
    let v = d.dot(qv) * inv;
    if v < -EPS_BARY || u + v > 1.0 + EPS_BARY {
        return None;
    }
    Some(e2.dot(qv) * inv)
}

/// Dreiecke aller Netze, nach ihrer Projektion in ein Raster der Bildebene einsortiert.
pub struct RefScene {
    tris: Vec<[DVec3; 3]>,
    basis: Basis,
    chord: f64,
    reach: f64,
    lo: DVec2,
    cell: DVec2,
    grid: Vec<Vec<u32>>,
    /// Merkmalskanten des Netzes in dieser Ansicht (Raum) und Netzkanten ohne genau zwei
    /// Dreiecke.
    feats: Vec<(DVec3, DVec3)>,
    open_edges: usize,
}

/// Urteil eines Referenzstrahls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RefRay {
    Free,
    Blocked,
    /// Ein Treffer liegt höchstens `4 · chord_tol` neben einer Merkmalskante des Netzes, oder
    /// der Strahlursprung liegt höchstens `4 · chord_tol` von der Ebene des getroffenen Dreiecks
    /// (streifender Strahl): Dort weicht das Netz um bis zu eine Sehnentoleranz vom Körper ab,
    /// der Treffer ist unsicher.
    Uncertain,
}

impl RefScene {
    /// Bereitet die Netze für die Ansicht `basis` vor; `extent` ist die Ausdehnung der Körper,
    /// die Netze haben die Sehnentoleranz `chord_tol(extent)`.
    pub fn new(meshes: &[Mesh], basis: Basis, extent: f64) -> RefScene {
        RefScene::with_chord(meshes, basis, extent, chord_tol(extent))
    }

    /// Wie [`RefScene::new`] für Netze mit der Sehnentoleranz `chord`.
    pub fn with_chord(meshes: &[Mesh], basis: Basis, extent: f64, chord: f64) -> RefScene {
        let tris: Vec<[DVec3; 3]> = meshes.iter().flat_map(triangles).collect();
        let proj = |p: DVec3| DVec2::new(p.dot(basis.ex), p.dot(basis.ey));
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        let mut lo3 = DVec3::splat(f64::INFINITY);
        let mut hi3 = DVec3::splat(f64::NEG_INFINITY);
        for t in &tris {
            for &p in t {
                lo = lo.min(proj(p));
                hi = hi.max(proj(p));
                lo3 = lo3.min(p);
                hi3 = hi3.max(p);
            }
        }
        let pad = DVec2::splat(1e-6 * extent);
        lo -= pad;
        hi += pad;
        let cell = ((hi - lo) / GRID as f64).max(DVec2::splat(1e-12));
        let mut grid = vec![Vec::new(); GRID * GRID];
        let s = RefScene {
            tris: Vec::new(),
            basis,
            chord,
            reach: (hi3 - lo3).length() + 2.0 * extent,
            lo,
            cell,
            grid: Vec::new(),
            feats: Vec::new(),
            open_edges: 0,
        };
        for (k, t) in tris.iter().enumerate() {
            let a = proj(t[0]).min(proj(t[1])).min(proj(t[2]));
            let b = proj(t[0]).max(proj(t[1])).max(proj(t[2]));
            let (i0, j0) = s.cell_of(a);
            let (i1, j1) = s.cell_of(b);
            for i in i0..=i1 {
                for j in j0..=j1 {
                    grid[i * GRID + j].push(k as u32);
                }
            }
        }
        let mut feats = Vec::new();
        let mut open_edges = 0;
        for m in meshes {
            let (f, open) = feature_edges_3d(m, &basis, extent);
            feats.extend(f);
            open_edges += open;
        }
        RefScene {
            tris,
            grid,
            feats,
            open_edges,
            ..s
        }
    }

    /// Merkmalskanten (Raum) aller Netze in dieser Ansicht.
    pub fn features(&self) -> &[(DVec3, DVec3)] {
        &self.feats
    }

    /// Netzkanten ohne genau zwei Dreiecke.
    pub fn open_edges(&self) -> usize {
        self.open_edges
    }

    /// Abstand des Punkts von der nächsten Merkmalskante.
    pub fn feature_dist(&self, x: DVec3) -> f64 {
        self.feats
            .iter()
            .map(|&(a, b)| seg_dist3(x, a, b).0)
            .fold(f64::INFINITY, f64::min)
    }

    fn cell_of(&self, p: DVec2) -> (usize, usize) {
        let q = (p - self.lo) / self.cell;
        let c = |x: f64| (x.floor().max(0.0) as usize).min(GRID - 1);
        (c(q.x), c(q.y))
    }

    /// Urteil des Strahls `o + s·d` (d normiert): Treffer zählen nur mit
    /// `5·chord_tol < s ≤ reach`; liegt einer davon nahe einer Merkmalskante, ist der Strahl
    /// unsicher, sonst verdeckt ihn jeder Treffer.
    pub fn cast(&self, o: DVec3, d: DVec3) -> RefRay {
        let proj = |p: DVec3| DVec2::new(p.dot(self.basis.ex), p.dot(self.basis.ey));
        let a = proj(o);
        let b = proj(o + d * self.reach);
        let (i0, j0) = self.cell_of(a.min(b));
        let (i1, j1) = self.cell_of(a.max(b));
        let min_s = 5.0 * self.chord;
        let near = 4.0 * self.chord;
        let mut out = RefRay::Free;
        for i in i0..=i1 {
            for j in j0..=j1 {
                for &k in &self.grid[i * GRID + j] {
                    let tri = &self.tris[k as usize];
                    if let Some(s) = ray_triangle(o, d, tri)
                        && s > min_s
                        && s <= self.reach
                    {
                        // Unsicher: Treffer nahe einer Merkmalskante, oder der Startpunkt liegt
                        // selbst innerhalb der Netzgenauigkeit an der Ebene des getroffenen
                        // Dreiecks (streifender Strahl längs einer Fläche – ob er das Netz
                        // trifft, entscheidet dann nur die Sehnenabweichung).
                        let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
                        let start_gap = (o - tri[0]).dot(n).abs();
                        if self.feature_dist(o + d * s) <= near || start_gap <= near {
                            return RefRay::Uncertain;
                        }
                        out = RefRay::Blocked;
                    }
                }
            }
        }
        out
    }

    /// Alle Treffer (Abstand, Dreieck) eines Strahls, ohne Abstandsgrenze (Fehlersuche).
    pub fn all_hits(&self, o: DVec3, d: DVec3) -> Vec<(f64, [DVec3; 3])> {
        self.tris
            .iter()
            .filter_map(|t| ray_triangle(o, d, t).map(|s| (s, *t)))
            .collect()
    }

    /// Eigene Kippabstimmung am Punkt `p`: 8 Strahlen zum Auge, je um `TILT_REF` in
    /// 45°-Schritten gekippt; ein freier Strahl → sichtbar, sonst mindestens ein verdeckter →
    /// verdeckt, sonst (alle unsicher) unentschieden.
    pub fn ref_visibility(&self, p: DVec3) -> Option<bool> {
        let eye = -self.basis.d;
        let mut blocked = false;
        for k in 0..TILT_REF_DIRECTIONS {
            let phi = k as f64 * std::f64::consts::TAU / TILT_REF_DIRECTIONS as f64;
            let d = (eye + (self.basis.ex * phi.cos() + self.basis.ey * phi.sin()) * TILT_REF)
                .normalize();
            if !d.is_finite() {
                continue;
            }
            match self.cast(p, d) {
                RefRay::Free => return Some(true),
                RefRay::Blocked => blocked = true,
                RefRay::Uncertain => {}
            }
        }
        blocked.then_some(false)
    }
}

/// Sichtbarkeit am Punkt `p` gegen das Netz `mesh` (Einzelaufruf; für viele Punkte
/// [`RefScene`] wiederverwenden).
pub fn ref_visibility(mesh: &Mesh, basis: &Basis, p: DVec3, extent: f64) -> Option<bool> {
    RefScene::new(std::slice::from_ref(mesh), *basis, extent).ref_visibility(p)
}

/// Merkmalskanten des Netzes im Raum: Kanten zwischen genau zwei Dreiecken, deren Normalen mehr
/// als 20° auseinanderliegen oder von denen eines zum Auge und eines davon weg zeigt (ein von
/// der Seite gesehenes Dreieck neben einem nicht von der Seite gesehenen zählt ebenso). Netzpunkte
/// werden über ihre auf `1e-7 · extent` gerundeten Koordinaten verschweißt (jede Fläche hat im
/// Netz eigene Punkte). Kanten mit einem oder mehr als zwei Dreiecken zählt `skipped`.
pub fn feature_edges_3d(mesh: &Mesh, basis: &Basis, extent: f64) -> (Vec<(DVec3, DVec3)>, usize) {
    let q = 1e-7 * extent;
    let key = |p: DVec3| {
        (
            (p.x / q).round() as i64,
            (p.y / q).round() as i64,
            (p.z / q).round() as i64,
        )
    };
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut pts: Vec<DVec3> = Vec::new();
    let mut weld = |p: DVec3| {
        *ids.entry(key(p)).or_insert_with(|| {
            pts.push(p);
            pts.len() - 1
        })
    };
    let mut normals: Vec<DVec3> = Vec::new();
    let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for t in triangles(mesh) {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        if n.length() == 0.0 || !n.is_finite() {
            continue;
        }
        let w = [weld(t[0]), weld(t[1]), weld(t[2])];
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue;
        }
        let k = normals.len();
        normals.push(n.normalize());
        for (a, b) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            edges.entry((a.min(b), a.max(b))).or_default().push(k);
        }
    }
    let cos_limit = FEATURE_ANGLE_DEG.to_radians().cos();
    let side = |n: DVec3| {
        let x = n.dot(basis.d);
        if x > EDGE_ON {
            1
        } else if x < -EDGE_ON {
            -1
        } else {
            0
        }
    };
    let mut out = Vec::new();
    let mut skipped = 0;
    for ((a, b), tri) in edges {
        if tri.len() != 2 {
            skipped += 1;
            continue;
        }
        let (n0, n1) = (normals[tri[0]], normals[tri[1]]);
        let sharp = n0.dot(n1) < cos_limit;
        // Vorder- gegen Rückseite; liegt genau auf dem Umriss ein von der Seite gesehenes
        // Dreieck (n·d = 0, etwa ein Streifen symmetrisch um die Umrisslinie), sind seine
        // Kanten zu Vorder- oder Rückseiten die Umrisskanten des Netzes.
        let (a0, a1) = (side(n0), side(n1));
        let turn = a0 * a1 < 0 || (a0 == 0) != (a1 == 0);
        if sharp || turn {
            out.push((pts[a], pts[b]));
        }
    }
    (out, skipped)
}

/// Merkmalskanten, projiziert (`(P·ex, P·ey)`).
pub fn feature_edges(mesh: &Mesh, basis: &Basis, extent: f64) -> Vec<(DVec2, DVec2)> {
    let proj = |p: DVec3| DVec2::new(p.dot(basis.ex), p.dot(basis.ey));
    feature_edges_3d(mesh, basis, extent)
        .0
        .into_iter()
        .map(|(a, b)| (proj(a), proj(b)))
        .collect()
}

/// Abstand des Punkts `x` von der Strecke `a b` im Raum und der nächste Streckenpunkt.
pub fn seg_dist3(x: DVec3, a: DVec3, b: DVec3) -> (f64, DVec3) {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 > 0.0 {
        ((x - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = a + ab * t;
    (q.distance(x), q)
}

/// Abstand des Punkts `p` von der Strecke `a b` (2D).
pub fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 > 0.0 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}
