//! `check_view`: prüft ein Ansichtsergebnis der Bibliothek gegen den eigenen Netz-Prüfer
//! (`mesh_ref`). Die Bibliothek liefert nur das zu prüfende Ergebnis; Referenzsichtbarkeit,
//! Merkmalskanten, Längen und Abstände rechnet dieses Modul selbst.

use super::mesh_ref::{RefScene, chord_tol, fine_mesh, seg_dist, seg_dist3};
use planora_drawing::model::ViewKind;
use planora_drawing::{Basis, DrawBody, ViewLines, Visibility, compute_view};
use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3};

/// Probepunkte je Stück für Abstände (dichte Abtastung).
const PIECE_SAMPLES: usize = 64;
/// Stützstellen der Längenmessung je Stück.
const LENGTH_SAMPLES: usize = 4096;

/// Ergebnis einer Prüfung: Fehler (Test rot), Warnungen (gezählt) und Kennzahlen.
#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Stellen, an denen Bibliothek und Referenz verschieden urteilen, die Referenz aber in der
    /// Nachbarschaft selbst wechselt (zulässig nach (e)).
    pub tolerated: usize,
    /// Geprüfte Stellen in (e).
    pub sampled: usize,
    /// Stellen, an denen die Referenz selbst unentschieden ist (alle Referenzstrahlen treffen
    /// nahe einer Merkmalskante des Netzes); dort ist keine Aussage möglich.
    pub undecided: usize,
    /// Zahl der Stücke und Merkmalskanten.
    pub pieces: usize,
    pub features: usize,
    /// Netzkanten ohne genau zwei Dreiecke (nicht als Merkmalskante gewertet).
    pub open_mesh_edges: usize,
}

/// Eigene Projektion `(P·ex, P·ey)`.
fn proj(b: &Basis, p: DVec3) -> DVec2 {
    DVec2::new(p.dot(b.ex), p.dot(b.ey))
}

/// Projizierte Länge eines Parameterbereichs: Mittelpunktsregel über `|p'(t)|`, `p'` als
/// symmetrischer Differenzenquotient.
fn proj_len(v: &ViewLines, stroke: usize, t0: f64, t1: f64) -> f64 {
    let s = &v.strokes[stroke];
    let b = &v.basis;
    let dt = (t1 - t0) / LENGTH_SAMPLES as f64;
    if dt == 0.0 {
        return 0.0;
    }
    let h = dt.abs() * 1e-3;
    let mut total = 0.0;
    for i in 0..LENGTH_SAMPLES {
        let t = t0 + dt * (i as f64 + 0.5);
        let d = (proj(b, s.curve.point(t + h)) - proj(b, s.curve.point(t - h))) / (2.0 * h);
        total += d.length() * dt.abs();
    }
    total
}

/// Summe der projizierten Längen aller Stücke der Sichtbarkeit `vis` (eigene Messung).
pub fn piece_length_sum(v: &ViewLines, vis: Visibility) -> f64 {
    v.pieces
        .iter()
        .filter(|p| p.vis == vis)
        .map(|p| proj_len(v, p.stroke, p.t0, p.t1))
        .sum()
}

/// Zahl der sichtbaren und der verdeckten Stücke.
pub fn counts(v: &ViewLines) -> (usize, usize) {
    let vis = v
        .pieces
        .iter()
        .filter(|p| p.vis == Visibility::Visible)
        .count();
    (vis, v.pieces.len() - vis)
}

/// Größte Kante der Hüllbox aus Eckpunkten und je 65 Punkten jeder Kante.
fn solid_extent(solids: &[&Solid]) -> f64 {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for s in solids {
        for (_, v) in s.vertices.iter() {
            lo = lo.min(v.point);
            hi = hi.max(v.point);
        }
        for (_, e) in s.edges.iter() {
            for k in 0..=64 {
                let p = e.curve.point(e.t0 + (e.t1 - e.t0) * k as f64 / 64.0);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    let d = hi - lo;
    d.x.max(d.y).max(d.z).max(1e-9)
}

/// Prüft die Ansicht `kind` der Körper gegen das Referenznetz:
/// (a) Abdeckung jedes Strichs durch seine Stücke, (b) keine zu kurzen Stücke (Warnung),
/// (c) jede Merkmalskante des Netzes liegt bei einem Stück, (d) jedes Stück liegt bei einer
/// Merkmalskante, (e) Sichtbarkeit wie die Referenz, außer wo die Referenz in der Nähe selbst
/// wechselt.
pub fn check_view(solids: &[&Solid], kind: ViewKind) -> Report {
    let mut r = Report::default();
    let bodies: Vec<DrawBody> = solids
        .iter()
        .enumerate()
        .map(|(i, s)| DrawBody {
            name: format!("K{i}"),
            solid: s,
        })
        .collect();
    let v = match compute_view(&bodies, kind, &Progress::new()) {
        Ok(v) => v,
        Err(e) => {
            r.errors.push(format!("compute_view: {e:?}"));
            return r;
        }
    };
    let basis = Basis::of(kind);
    // Eigene Ausdehnung (Eckpunkte und Kantenpunkte), dann das feine Referenznetz.
    let extent = solid_extent(solids);
    let chord = chord_tol(extent);
    let near = 4.0 * chord;
    let meshes: Vec<_> = solids.iter().map(|s| fine_mesh(s, extent)).collect();
    let scene = RefScene::new(&meshes, basis, extent);
    let feats3 = scene.features();
    r.open_mesh_edges = scene.open_edges();
    let feats: Vec<(DVec2, DVec2)> = feats3
        .iter()
        .map(|&(a, b)| (proj(&basis, a), proj(&basis, b)))
        .collect();
    // Die Referenz prüft die Netzfassung der Linie: den nächsten Punkt einer Merkmalskante des
    // Netzes im Raum, wenn einer höchstens `near` entfernt ist. Sonst läge der exakte
    // Linienpunkt an hohlen Rundungen (Bohrungsrand) um bis zu eine Sehnentoleranz im Material
    // des Netzes, und die Referenz sähe die Linie als verdeckt, obwohl sie es im Netz nicht ist.
    let snap = |x: DVec3| -> DVec3 {
        let mut best = (near, x);
        for &(a, b) in feats3 {
            let (d, q) = seg_dist3(x, a, b);
            if d <= best.0 {
                best = (d, q);
            }
        }
        best.1
    };
    let ref_at = |x: DVec3| scene.ref_visibility(snap(x));
    r.features = feats.len();
    r.pieces = v.pieces.len();

    // (a) Abdeckung.
    for (i, s) in v.strokes.iter().enumerate() {
        let (lo, hi) = (s.t0.min(s.t1), s.t0.max(s.t1));
        let mut ps: Vec<_> = v.pieces.iter().filter(|p| p.stroke == i).collect();
        ps.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        let same = |x: f64, y: f64| (x - y).abs() <= 1e-12 * (1.0 + x.abs().max(y.abs()));
        let ok = !ps.is_empty()
            && same(ps[0].t0, lo)
            && same(ps[ps.len() - 1].t1, hi)
            && ps.windows(2).all(|w| same(w[0].t1, w[1].t0))
            && ps.iter().all(|p| p.t1 > p.t0);
        if !ok {
            let ranges: Vec<_> = ps.iter().map(|p| (p.t0, p.t1)).collect();
            r.errors.push(format!(
                "(a) Strich {i} [{lo}, {hi}] nicht lückenlos bedeckt: {ranges:?}"
            ));
        }
    }

    // Dichte Abtastung der Stücke (2D) mit Hüllbox.
    let polys: Vec<(Vec<DVec2>, DVec2, DVec2)> = v
        .pieces
        .iter()
        .map(|p| {
            let s = &v.strokes[p.stroke];
            let pts: Vec<DVec2> = (0..PIECE_SAMPLES)
                .map(|k| {
                    let t = p.t0 + (p.t1 - p.t0) * k as f64 / (PIECE_SAMPLES - 1) as f64;
                    proj(&basis, s.curve.point(t))
                })
                .collect();
            let lo = pts
                .iter()
                .fold(DVec2::splat(f64::INFINITY), |a, &b| a.min(b));
            let hi = pts
                .iter()
                .fold(DVec2::splat(f64::NEG_INFINITY), |a, &b| a.max(b));
            (pts, lo, hi)
        })
        .collect();

    // (b) Mindestlänge.
    let gtol = (64.0 * f64::EPSILON * extent).max(1e-9);
    for (k, p) in v.pieces.iter().enumerate() {
        let l = proj_len(&v, p.stroke, p.t0, p.t1);
        if l < 64.0 * gtol {
            r.warnings.push(format!(
                "(b) Stück {k} (Strich {}) ist nur {l:e} mm lang",
                p.stroke
            ));
        }
    }

    // (c) Vollständigkeit.
    let dist_to_pieces = |q: DVec2| -> f64 {
        let mut best = f64::INFINITY;
        for (pts, lo, hi) in &polys {
            if q.x < lo.x - near || q.x > hi.x + near || q.y < lo.y - near || q.y > hi.y + near {
                continue;
            }
            for w in pts.windows(2) {
                best = best.min(seg_dist(q, w[0], w[1]));
            }
        }
        best
    };
    for (a, b) in &feats {
        for f in [0.25, 0.5, 0.75] {
            let q = a.lerp(*b, f);
            let d = dist_to_pieces(q);
            if d > near {
                r.errors.push(format!(
                    "(c) Merkmalskante bei ({:.4}, {:.4}) fehlt (Abstand {d:e})",
                    q.x, q.y
                ));
            }
        }
    }

    // (d) Keine Phantomlinien.
    for (k, p) in v.pieces.iter().enumerate() {
        let s = &v.strokes[p.stroke];
        let m = proj(&basis, s.curve.point(0.5 * (p.t0 + p.t1)));
        let d = feats
            .iter()
            .map(|(a, b)| seg_dist(m, *a, *b))
            .fold(f64::INFINITY, f64::min);
        if d > near {
            r.errors.push(format!(
                "(d) Stück {k} (Strich {}, {:?}) bei ({:.4}, {:.4}) ohne Merkmalskante (Abstand {d:e})",
                p.stroke, s.origin, m.x, m.y
            ));
        }
    }

    // (e) Sichtbarkeit.
    for (k, p) in v.pieces.iter().enumerate() {
        let s = &v.strokes[p.stroke];
        let lib = p.vis == Visibility::Visible;
        for f in [0.25, 0.5, 0.75] {
            let t = p.t0 + f * (p.t1 - p.t0);
            let x = s.curve.point(t);
            r.sampled += 1;
            let want = ref_at(x);
            if want == Some(lib) {
                continue;
            }
            if want.is_none() {
                r.undecided += 1;
                continue;
            }
            let h = 1e-6 * (p.t1 - p.t0).abs().max(1e-12);
            let tan = (s.curve.point(t + h) - s.curve.point(t - h)).normalize_or_zero();
            let changes = [x + tan * near, x - tan * near]
                .iter()
                .any(|&y| ref_at(y) != want);
            if changes {
                r.tolerated += 1;
            } else {
                r.errors.push(format!(
                    "(e) Stück {k} (Strich {}, {:?}) bei t = {t} ({:.4}, {:.4}, {:.4}): Bibliothek {:?}, Referenz {want:?}",
                    p.stroke, s.origin, x.x, x.y, x.z, p.vis
                ));
            }
        }
    }
    r
}
