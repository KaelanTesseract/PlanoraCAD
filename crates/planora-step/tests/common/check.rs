//! Prüfungen aus den gelesenen Daten (Spec §5.2, §5.3) und Vergleich mit dem Kernel: Topologie
//! und Euler-Zahl je Hülle, Eckpunkte auf ihren Kurven, Kanten auf beiden Nachbarflächen,
//! Punktprobe Kernel ↔ STEP-Fläche mit Normalenrichtung, Netz aus den STEP-Flächen.

use super::model::{Crv, EdgeM, FaceM, Model, Oriented, ShellM, Surf};
use planora_kernel::{Face, Id, Solid};
use planora_math::DVec3;
use planora_mesh::{TessellationOptions, tessellate};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Lagetoleranz für analytische Kurven und Flächen (mm).
pub const GEOM_TOL: f64 = 1e-7;
/// Lagetoleranz, wenn eine Freiformkurve oder -fläche beteiligt ist (mm); über der des Kernels
/// (`NURBS_EDGE_TOL` = 1e-5), weil Kanten dort nur so genau auf ihren Flächen liegen.
pub const FREEFORM_TOL: f64 = 2e-5;
/// Abschnitte, an deren Grenzen eine Kante geprüft wird (Enden eingeschlossen).
pub const EDGE_SAMPLES: usize = 8;
/// Raster der Punktprobe auf gekrümmten Flächen: `(n + 1)²` Punkte über den Parameterbereich.
pub const PATCH_SAMPLES: usize = 6;
/// Mindestlänge einer Normale, damit ihre Richtung zählt (an Polen und Spitzen kürzer).
pub const NORMAL_MIN: f64 = 0.5;
/// Mindestwert des Skalarprodukts zweier Einheitsnormalen, die dieselbe Seite meinen.
pub const NORMAL_AGREE: f64 = 0.9;

/// Kennzahlen einer Hülle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellStats {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    /// Innere Ränder (Löcher in Flächen).
    pub rings: usize,
    pub genus: usize,
}

/// Toleranz für eine Kurve.
fn curve_tol(c: &Crv) -> f64 {
    if matches!(c, Crv::Spline(_)) {
        FREEFORM_TOL
    } else {
        GEOM_TOL
    }
}

/// Orientierte Kanten eines Randes in Laufrichtung (`orientation = .F.` kehrt um).
fn walk(edges: &[Oriented], orientation: bool) -> Vec<Oriented> {
    if orientation {
        edges.to_vec()
    } else {
        edges
            .iter()
            .rev()
            .map(|o| Oriented {
                edge: o.edge,
                forward: !o.forward,
            })
            .collect()
    }
}

/// Topologie je Hülle (Spec §5.3): jede Kante genau einmal vorwärts und einmal rückwärts, jede
/// Schleife geschlossen, keine Kante in zwei Hüllen, Euler-Poincaré
/// `V − E + F − R = 2 − 2g` mit ganzzahligem `g ≥ 0`.
pub fn check_topology(m: &Model) -> Result<Vec<ShellStats>, String> {
    let mut owner: HashMap<u64, u64> = HashMap::new();
    let mut out = Vec::new();
    for shell in m.shells() {
        let mut uses: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
        let mut rings = 0;
        for face in &shell.faces {
            rings += face.bounds.len() - 1;
            for bound in &face.bounds {
                let seq = walk(&bound.edges, bound.orientation);
                for (i, o) in seq.iter().enumerate() {
                    let u = uses.entry(o.edge).or_default();
                    if o.forward {
                        u.0 += 1;
                    } else {
                        u.1 += 1;
                    }
                    let next = &seq[(i + 1) % seq.len()];
                    if m.ends(o).1 != m.ends(next).0 {
                        return Err(format!("Fläche #{}: Schleife nicht geschlossen", face.id));
                    }
                }
            }
        }
        let mut verts = BTreeSet::new();
        for (edge, (f, b)) in &uses {
            if (*f, *b) != (1, 1) {
                return Err(format!(
                    "Kante #{edge}: {f}-mal vorwärts, {b}-mal rückwärts (erwartet je einmal)"
                ));
            }
            if let Some(other) = owner.insert(*edge, shell.id) {
                return Err(format!(
                    "Kante #{edge} in den Hüllen #{other} und #{}",
                    shell.id
                ));
            }
            let e = &m.edges[edge];
            verts.insert(e.start);
            verts.insert(e.end);
        }
        let chi = verts.len() as i64 - uses.len() as i64 + shell.faces.len() as i64 - rings as i64;
        let twice_genus = 2 - chi;
        if twice_genus < 0 || twice_genus % 2 != 0 {
            return Err(format!("Hülle #{}: Euler-Zahl {chi} unmöglich", shell.id));
        }
        out.push(ShellStats {
            vertices: verts.len(),
            edges: uses.len(),
            faces: shell.faces.len(),
            rings,
            genus: (twice_genus / 2) as usize,
        });
    }
    Ok(out)
}

/// Parameterbereich einer Kante auf ihrer STEP-Kurve: vom Anfangs- zum Endpunkt in
/// Kurvenrichtung (`same_sense`) bzw. dagegen; Vollkreise (gleicher Eckpunkt) eine ganze
/// Periode.
pub fn edge_range(m: &Model, e: &EdgeM) -> Result<(f64, f64), String> {
    let (p0, p1) = (m.vertices[&e.start], m.vertices[&e.end]);
    let t0 = e.curve.param(p0);
    let raw = e.curve.param(p1);
    let t1 = match e.curve.period() {
        Some(per) if e.start == e.end => {
            if e.same_sense {
                t0 + per
            } else {
                t0 - per
            }
        }
        Some(per) => {
            if e.same_sense {
                t0 + (raw - t0).rem_euclid(per)
            } else {
                t0 - (t0 - raw).rem_euclid(per)
            }
        }
        None => {
            if (raw > t0) != e.same_sense && raw != t0 {
                return Err("Kante läuft gegen die Richtung ihres same_sense".into());
            }
            raw
        }
    };
    Ok((t0, t1))
}

/// Punkte einer Kante (Enden eingeschlossen) auf der STEP-Kurve.
pub fn edge_points(m: &Model, e: &EdgeM) -> Result<Vec<DVec3>, String> {
    let (t0, t1) = edge_range(m, e)?;
    Ok((0..=EDGE_SAMPLES)
        .map(|k| {
            e.curve
                .point(t0 + (t1 - t0) * k as f64 / EDGE_SAMPLES as f64)
        })
        .collect())
}

/// Geometrie aus den gelesenen Daten (Spec §5.2): jeder Eckpunkt liegt auf seinen Kurven, jede
/// Kante auf allen Flächen, die sie begrenzt; die Enden der Kurvenstücke treffen die Eckpunkte.
pub fn check_geometry(m: &Model) -> Result<(), String> {
    let mut samples: BTreeMap<u64, Vec<DVec3>> = BTreeMap::new();
    for (id, e) in &m.edges {
        let tol = curve_tol(&e.curve);
        for v in [e.start, e.end] {
            let d = e.curve.distance(m.vertices[&v]);
            if d > tol {
                return Err(format!("Eckpunkt #{v} liegt {d} neben der Kurve von #{id}"));
            }
        }
        let pts = edge_points(m, e)?;
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        if first.distance(m.vertices[&e.start]) > tol || last.distance(m.vertices[&e.end]) > tol {
            return Err(format!(
                "Kante #{id}: Kurvenstück trifft die Eckpunkte nicht"
            ));
        }
        samples.insert(*id, pts);
    }
    for shell in m.shells() {
        for face in &shell.faces {
            let face_tol = if face.surface.is_spline() {
                FREEFORM_TOL
            } else {
                GEOM_TOL
            };
            for bound in &face.bounds {
                for o in &bound.edges {
                    let tol = face_tol.max(curve_tol(&m.edges[&o.edge].curve));
                    for p in &samples[&o.edge] {
                        let d = face.surface.distance(*p);
                        if d > tol {
                            return Err(format!(
                                "Kante #{} liegt {d} neben der Fläche #{}",
                                o.edge, face.id
                            ));
                        }
                    }
                }
            }
            check_closed_surface(face)?;
        }
    }
    for (id, e) in &m.edges {
        if let Crv::Spline(s) = &e.curve
            && s.closed
        {
            let (a, b) = s.domain();
            let gap = s.point(a).distance(s.point(b));
            if gap > FREEFORM_TOL {
                return Err(format!("Kurve von #{id} heißt geschlossen, Lücke {gap}"));
            }
        }
    }
    Ok(())
}

/// Als geschlossen gekennzeichnete Freiformflächen haben gleiche erste und letzte
/// Kontrollpunktreihe in dieser Richtung.
fn check_closed_surface(face: &FaceM) -> Result<(), String> {
    let Surf::Spline(s) = &face.surface else {
        return Ok(());
    };
    let (nu, nv) = s.counts();
    let u_ok = (0..nv).all(|j| s.control[0][j].distance(s.control[nu - 1][j]) <= FREEFORM_TOL);
    let v_ok = (0..nu).all(|i| s.control[i][0].distance(s.control[i][nv - 1]) <= FREEFORM_TOL);
    if (s.u_closed && !u_ok) || (s.v_closed && !v_ok) {
        return Err(format!(
            "Fläche #{}: als geschlossen gekennzeichnet, ist es aber nicht",
            face.id
        ));
    }
    Ok(())
}

/// Flächen der Schalen eines Kernel-Körpers (ohne Schalenliste alle Flächen als eine).
pub fn kernel_shells(solid: &Solid) -> Vec<Vec<Id<Face>>> {
    if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    }
}

/// Erster Eckpunkt einer Hülle: Anfang der ersten Kante im äußeren Rand der ersten Fläche.
fn first_point_step(m: &Model, s: &ShellM) -> DVec3 {
    let b = &s.faces[0].bounds[0];
    let seq = walk(&b.edges, b.orientation);
    m.vertices[&m.ends(&seq[0]).0]
}

fn first_point_kernel(solid: &Solid, faces: &[Id<Face>]) -> DVec3 {
    let he = solid.faces[faces[0]].outer.half_edges[0];
    solid.vertices[solid.half_edge_start(he)].point
}

/// Zuordnung STEP-Fläche → Kernel-Fläche für einen Körper: Hüllen über Flächenzahl und ersten
/// Eckpunkt (bitgleich, weil die Zahlen verlustfrei geschrieben werden), Flächen in Reihenfolge.
pub fn face_map<'m>(
    m: &'m Model,
    product: usize,
    solid: &Solid,
) -> Result<Vec<(&'m FaceM, Id<Face>)>, String> {
    let shells: Vec<&ShellM> = m.products[product]
        .solids
        .iter()
        .flat_map(|s| std::iter::once(&s.outer).chain(s.voids.iter()))
        .collect();
    let kernel = kernel_shells(solid);
    if shells.len() != kernel.len() {
        return Err(format!("{} Hüllen statt {}", shells.len(), kernel.len()));
    }
    let mut used = vec![false; kernel.len()];
    let mut out = Vec::new();
    for s in shells {
        let p = first_point_step(m, s);
        let k = (0..kernel.len())
            .find(|&k| {
                !used[k]
                    && kernel[k].len() == s.faces.len()
                    && first_point_kernel(solid, &kernel[k]) == p
            })
            .ok_or_else(|| format!("Hülle #{} passt zu keiner Schale des Kernels", s.id))?;
        used[k] = true;
        out.extend(s.faces.iter().zip(kernel[k].iter().copied()));
    }
    Ok(out)
}

/// Probepunkte einer Kernel-Fläche mit äußerer Normale: Raster über den Parameterbereich bzw.
/// bei Ebenen die Eckpunkte der Schleifen.
fn kernel_probes(solid: &Solid, id: Id<Face>) -> Vec<(DVec3, DVec3)> {
    let f = &solid.faces[id];
    let sign = if f.same_sense { 1.0 } else { -1.0 };
    match f.patch {
        Some(r) => {
            let mut out = Vec::new();
            for i in 0..=PATCH_SAMPLES {
                for j in 0..=PATCH_SAMPLES {
                    let u = r.u0 + (r.u1 - r.u0) * i as f64 / PATCH_SAMPLES as f64;
                    let v = r.v0 + (r.v1 - r.v0) * j as f64 / PATCH_SAMPLES as f64;
                    out.push((f.surface.point(u, v), f.surface.normal(u, v) * sign));
                }
            }
            out
        }
        None => std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .flat_map(|lp| lp.half_edges.iter())
            .map(|he| {
                let p = solid.vertices[solid.half_edge_start(*he)].point;
                let (u, v) = f.surface.project(p);
                (p, f.surface.normal(u, v) * sign)
            })
            .collect(),
    }
}

/// Vergleich mit dem Kernel (Spec §5.2): je Teil ein Körper in Reihenfolge; gleiche Zahl von
/// Rändern und Kanten, gleiche Richtungen, bitgleiche Eckpunkte; Punkte der Kernel-Kanten auf
/// den STEP-Kurven (Laufrichtung gleich), Punkte der Kernel-Flächen auf den STEP-Flächen mit
/// gleicher Außennormale (auch bei gespiegelten und gedrehten Körpern).
pub fn compare_with_kernel(m: &Model, solids: &[&Solid]) -> Result<(), String> {
    if m.products.len() != solids.len() {
        return Err(format!("{} Teile statt {}", m.products.len(), solids.len()));
    }
    for (i, solid) in solids.iter().enumerate() {
        for (fm, fid) in face_map(m, i, solid)? {
            compare_face(m, fm, solid, fid)?;
        }
    }
    Ok(())
}

fn compare_face(m: &Model, fm: &FaceM, solid: &Solid, fid: Id<Face>) -> Result<(), String> {
    let kf = &solid.faces[fid];
    let loops: Vec<_> = std::iter::once(&kf.outer).chain(kf.inner.iter()).collect();
    if fm.bounds.len() != loops.len() || !fm.bounds[0].outer {
        return Err(format!("Fläche #{}: Ränder passen nicht", fm.id));
    }
    for (b, lp) in fm.bounds.iter().zip(&loops) {
        if !b.orientation || b.edges.len() != lp.half_edges.len() {
            return Err(format!("Fläche #{}: Schleife passt nicht", fm.id));
        }
        for (o, he) in b.edges.iter().zip(&lp.half_edges) {
            if o.forward != he.forward {
                return Err(format!(
                    "Kante #{}: Richtung in der Schleife falsch",
                    o.edge
                ));
            }
            let e = &m.edges[&o.edge];
            let ke = &solid.edges[he.edge];
            let (ks, kend) = (solid.vertices[ke.start].point, solid.vertices[ke.end].point);
            if m.vertices[&e.start] != ks || m.vertices[&e.end] != kend {
                return Err(format!("Kante #{}: Eckpunkte nicht bitgleich", o.edge));
            }
            let tol = curve_tol(&e.curve);
            for k in 0..=EDGE_SAMPLES {
                let t = ke.t0 + (ke.t1 - ke.t0) * k as f64 / EDGE_SAMPLES as f64;
                let d = e.curve.distance(ke.curve.point(t));
                if d > tol {
                    return Err(format!(
                        "Kante #{}: Kernelpunkt {d} neben der Kurve",
                        o.edge
                    ));
                }
            }
            let kt = ke.curve.tangent(ke.t0) * (ke.t1 - ke.t0).signum();
            let st = e.curve.tangent(e.curve.param(ks)) * if e.same_sense { 1.0 } else { -1.0 };
            if kt.dot(st) <= 0.0 {
                return Err(format!("Kante #{}: läuft in STEP andersherum", o.edge));
            }
        }
    }
    let tol = if fm.surface.is_spline() {
        FREEFORM_TOL
    } else {
        GEOM_TOL
    };
    let sign = if fm.same_sense { 1.0 } else { -1.0 };
    for (p, nk) in kernel_probes(solid, fid) {
        let d = fm.surface.distance(p);
        if d > tol {
            return Err(format!(
                "Fläche #{}: Kernelpunkt {p} liegt {d} daneben",
                fm.id
            ));
        }
        let ns = fm.surface.normal_at(p) * sign;
        if nk.length() >= NORMAL_MIN && ns.length() >= NORMAL_MIN {
            let agree = nk.normalize().dot(ns.normalize());
            if agree < NORMAL_AGREE {
                return Err(format!(
                    "Fläche #{}: Außennormale bei {p} zeigt anders ({agree})",
                    fm.id
                ));
            }
        }
    }
    Ok(())
}

/// Maße aus den STEP-Flächen (Spec §5.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    /// Volumen und Fläche des Netzes mit Punkten auf den STEP-Flächen und Dreiecken nach deren
    /// Außennormale ausgerichtet.
    pub volume: f64,
    pub area: f64,
    /// Volumen desselben Netzes mit den Punkten und Richtungen des Kernels.
    pub kernel_volume: f64,
    /// Dreiecke, die nach der STEP-Normale umgedreht werden mussten (richtig: 0).
    pub flipped: usize,
    /// Größter Abstand eines Netzpunkts des Kernels von seiner STEP-Fläche.
    pub max_offset: f64,
}

/// Netz aus den gelesenen STEP-Flächen: Die Dreiecke des Kernel-Netzes (Sehnentoleranz `chord`)
/// geben nur die Verbindung vor; jeder Punkt wird auf die STEP-Fläche seiner Fläche gelegt, jedes
/// Dreieck nach deren Außennormale (natürliche Normale × `same_sense`) ausgerichtet.
pub fn measure(m: &Model, solids: &[&Solid], chord: f64) -> Result<Measure, String> {
    let mut out = Measure {
        volume: 0.0,
        area: 0.0,
        kernel_volume: 0.0,
        flipped: 0,
        max_offset: 0.0,
    };
    for (i, solid) in solids.iter().enumerate() {
        let faces: HashMap<Id<Face>, &FaceM> = face_map(m, i, solid)?
            .into_iter()
            .map(|(f, id)| (id, f))
            .collect();
        let mesh = tessellate(
            solid,
            &TessellationOptions {
                chord_tolerance: chord,
            },
        )
        .map_err(|e| format!("Netz: {e}"))?;
        let (lo, hi) = mesh
            .positions
            .iter()
            .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
        let o = 0.5 * (lo + hi);
        for r in &mesh.face_ranges {
            let fm = faces[&r.face];
            let sign = if fm.same_sense { 1.0 } else { -1.0 };
            let start = r.first_index as usize;
            let end = start + r.index_count as usize;
            // Fußpunkt und Normale je Netzpunkt einmal; der letzte Fußpunkt dient als Startwert.
            let mut feet: HashMap<u32, (DVec3, DVec3)> = HashMap::new();
            let mut hint = None;
            for t in mesh.indices[start..end].as_chunks::<3>().0 {
                let k = t.map(|i| mesh.positions[i as usize]);
                out.kernel_volume += (k[0] - o).dot((k[1] - o).cross(k[2] - o)) / 6.0;
                let f = t.map(|i| {
                    *feet
                        .entry(i)
                        .or_insert_with(|| fm.surface.foot(mesh.positions[i as usize], &mut hint))
                });
                for (a, (b, _)) in k.iter().zip(&f) {
                    out.max_offset = out.max_offset.max(a.distance(*b));
                }
                let s = f.map(|(p, _)| p);
                let n = (s[1] - s[0]).cross(s[2] - s[0]);
                let ns = (f[0].1 + f[1].1 + f[2].1) * sign;
                let flip = n.length() > 0.0 && ns.length() >= NORMAL_MIN && n.dot(ns) < 0.0;
                let (a, b, c) = if flip {
                    out.flipped += 1;
                    (s[0], s[2], s[1])
                } else {
                    (s[0], s[1], s[2])
                };
                out.volume += (a - o).dot((b - o).cross(c - o)) / 6.0;
                out.area += 0.5 * n.length();
            }
        }
    }
    Ok(out)
}
