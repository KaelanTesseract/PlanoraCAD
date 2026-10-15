//! `check_view`: prüft ein Ansichtsergebnis der Bibliothek gegen den eigenen Netz-Prüfer
//! (`mesh_ref`). Die Bibliothek liefert nur das zu prüfende Ergebnis; Referenzsichtbarkeit,
//! Merkmalskanten, Längen und Abstände rechnet dieses Modul selbst.

use super::mesh_ref::{RefScene, chord_tol, fine_mesh, mesh_with_chord, seg_dist, seg_dist3};
use planora_drawing::model::ViewKind;
use planora_drawing::{Basis, DrawBody, ViewLines, Visibility, compute_view};
use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3};
use std::cell::OnceCell;

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
    /// Stellen, die ein feineres Netz nachprüfen musste (Grundnetz unentschieden bzw.
    /// abweichend ohne eigenen Wechsel in der Nähe).
    pub refined_undecided: usize,
    pub refined_differs: usize,
    /// Stellen von Merkmalskanten des Grundnetzes ohne Stück, die das feinere Netz nicht
    /// bestätigt (Fehler des groben Netzes).
    pub refined_features: usize,
    /// Zahl der Stücke und Merkmalskanten.
    pub pieces: usize,
    pub features: usize,
    /// Netzkanten ohne genau zwei Dreiecke (nicht als Merkmalskante gewertet).
    pub open_mesh_edges: usize,
    /// Je Fehler aus (e): Index in `errors`, Raumpunkt und Urteil der Bibliothek (sichtbar?),
    /// damit ein Aufrufer mit einem genaueren eigenen Richter nachprüfen kann.
    pub vis_diffs: Vec<(usize, DVec3, bool)>,
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
/// (a) Abdeckung jedes Strichs durch seine Stücke oder deckungsgleiche Stücke anderer Striche,
/// (b) keine zu kurzen Stücke (Fehler),
/// (c) jede Merkmalskante des Netzes liegt bei einem Stück, (d) jedes Stück liegt bei einer
/// Merkmalskante, (e) Sichtbarkeit wie die Referenz, außer wo die Referenz in der Nähe selbst
/// wechselt.
pub fn check_view(solids: &[&Solid], kind: ViewKind) -> Report {
    let bodies: Vec<DrawBody> = solids
        .iter()
        .enumerate()
        .map(|(i, s)| DrawBody {
            name: format!("K{i}"),
            solid: s,
        })
        .collect();
    match compute_view(&bodies, kind, &Progress::new()) {
        Ok(v) => check_lines(solids, kind, &v),
        Err(e) => Report {
            errors: vec![format!("compute_view: {e:?}")],
            ..Report::default()
        },
    }
}

/// Wie [`check_view`] für ein schon berechnetes Ergebnis `v` der Ansicht `kind`.
pub fn check_lines(solids: &[&Solid], kind: ViewKind, v: &ViewLines) -> Report {
    let mut r = Report::default();
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

    r.features = feats.len();
    r.pieces = v.pieces.len();

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

    // (b) Mindestlänge: Nach dem Zusammenfassen gibt es keine Stücke unter 64·geom_tol.
    let gtol = (64.0 * f64::EPSILON * extent).max(1e-9);
    for (k, p) in v.pieces.iter().enumerate() {
        let l = proj_len(v, p.stroke, p.t0, p.t1);
        if l < 64.0 * gtol {
            r.errors.push(format!(
                "(b) Stück {k} (Strich {}) ist nur {l:e} mm lang",
                p.stroke
            ));
        }
    }

    // Feinere Netze (Sehnentoleranz `1e-4 · E`, dann `1e-5 · E`) für Nachprüfungen in (c) und (e).
    let finer: [OnceCell<RefScene>; 2] = [OnceCell::new(), OnceCell::new()];
    let level = |i: usize| -> &RefScene {
        if i == 0 {
            return &scene;
        }
        finer[i - 1].get_or_init(|| {
            let c = FINER_CHORD_REL[i - 1] * extent;
            let m: Vec<_> = solids.iter().map(|s| mesh_with_chord(s, c)).collect();
            RefScene::with_chord(&m, basis, extent, c)
        })
    };
    let chords = [
        chord,
        FINER_CHORD_REL[0] * extent,
        FINER_CHORD_REL[1] * extent,
    ];

    // (c) Vollständigkeit. Fehlt bei einer Merkmalskante des Grundnetzes ein Stück, prüft das
    // feinere Netz (1e-4·E) nach: Hat es dort (im Abstand `near`) keine Merkmalskante, war es
    // ein Fehler des groben Netzes (etwa lange Dreiecke am Pol einer beschnittenen Kugel).
    // Abstand zum nächsten Stück und zum nächsten sichtbaren Stück.
    let dist_to = |q: DVec2, only_visible: bool| -> f64 {
        let mut best = f64::INFINITY;
        for ((pts, lo, hi), p) in polys.iter().zip(&v.pieces) {
            if only_visible && p.vis != Visibility::Visible {
                continue;
            }
            if q.x < lo.x - near || q.x > hi.x + near || q.y < lo.y - near || q.y > hi.y + near {
                continue;
            }
            for w in pts.windows(2) {
                best = best.min(seg_dist(q, w[0], w[1]));
            }
        }
        best
    };
    let dist_to_pieces = |q: DVec2| dist_to(q, false);
    for (a, b) in &feats {
        for f in [0.25, 0.5, 0.75] {
            let q = a.lerp(*b, f);
            let d = dist_to_pieces(q);
            let fine_has = || {
                level(1)
                    .features()
                    .iter()
                    .any(|&(a3, b3)| seg_dist(q, proj(&basis, a3), proj(&basis, b3)) <= near)
            };
            if d > near && !fine_has() {
                r.refined_features += 1;
            } else if d > near {
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

    // (a) Abdeckung: Die Stücke eines Strichs liegen in seinem Bereich (geschlossene Striche
    // dürfen über die Naht reichen), aufsteigend und ohne Überlappung. Lücken sind nur erlaubt,
    // wo das Zusammenfassen ein Stück entfernt hat: im Bild kürzer als 128·geom_tol (winziges
    // Stück), oder jede Stelle der Lücke liegt bei einem anderen Stück (deckungsgleich); ist die
    // Referenz an der Stelle sichtbar, muss es ein sichtbares sein (sichtbar gewinnt).
    let gap_samples = 8;
    for (i, s) in v.strokes.iter().enumerate() {
        let (lo, hi) = (s.t0.min(s.t1), s.t0.max(s.t1));
        let period = s.curve.period().filter(|p| (hi - lo - p).abs() <= 1e-9 * p);
        let mut ivs: Vec<(f64, f64)> = Vec::new();
        let mut bad = false;
        for p in v.pieces.iter().filter(|p| p.stroke == i) {
            bad |= p.t1 <= p.t0 || p.t0 < lo - 1e-12 * (1.0 + lo.abs());
            match period {
                Some(per) if p.t1 > hi => {
                    bad |= p.t1 - per > p.t0;
                    ivs.push((p.t0, hi));
                    ivs.push((lo, p.t1 - per));
                }
                _ => {
                    bad |= p.t1 > hi + 1e-12 * (1.0 + hi.abs());
                    ivs.push((p.t0, p.t1));
                }
            }
        }
        ivs.sort_by(|a, b| a.0.total_cmp(&b.0));
        bad |= ivs
            .windows(2)
            .any(|w| w[1].0 < w[0].1 - 1e-12 * (1.0 + w[0].1.abs()));
        if bad {
            r.errors.push(format!(
                "(a) Strich {i} [{lo}, {hi}]: Stücke überlappen oder liegen außerhalb: {ivs:?}"
            ));
            continue;
        }
        let mut gaps = Vec::new();
        let mut at = lo;
        for &(a, b) in &ivs {
            if a > at {
                gaps.push((at, a));
            }
            at = at.max(b);
        }
        if hi > at {
            gaps.push((at, hi));
        }
        for (g0, g1) in gaps {
            if proj_len(v, i, g0, g1) < 128.0 * gtol {
                continue;
            }
            for k in 0..gap_samples {
                let t = g0 + (g1 - g0) * (k as f64 + 0.5) / gap_samples as f64;
                let x = s.curve.point(t);
                let q = proj(&basis, x);
                if dist_to(q, false) > near {
                    r.errors.push(format!(
                        "(a) Strich {i}: Lücke [{g0}, {g1}] bei ({:.4}, {:.4}) ohne Stück",
                        q.x, q.y
                    ));
                    break;
                }
                if dist_to(q, true) > near {
                    let h = 1e-6 * (g1 - g0).abs().max(1e-12);
                    let tan = (s.curve.point(t + h) - s.curve.point(t - h)).normalize_or_zero();
                    if let Judged::Differs(true) = judge(&scene, 4.0 * chord, near, x, tan, false) {
                        r.errors.push(format!(
                            "(a) Strich {i}: Lücke [{g0}, {g1}] bei ({:.4}, {:.4}) sichtbar, aber nur verdeckt bedeckt",
                            q.x, q.y
                        ));
                        break;
                    }
                }
            }
        }
    }

    // (e) Sichtbarkeit: jedes Stück dicht abgetastet (ohne ein Band von `near` an den Enden),
    // damit auch ein Wechsel mitten in einem angeblich einheitlichen Stück auffällt. Bleibt die
    // Referenz unentschieden oder weicht sie ab, ohne dass sie selbst in der Nähe wechselt,
    // entscheidet ein feineres Netz (Sehnentoleranz `1e-4 · E`, dann `1e-5 · E`); die
    // Wechselprüfung längs der Linie behält das Band `near` des Grundnetzes.
    for (k, p) in v.pieces.iter().enumerate() {
        let s = &v.strokes[p.stroke];
        let lib = p.vis == Visibility::Visible;
        let (a, b) = (s.curve.point(p.t0), s.curve.point(p.t1));
        for i in 0..VIS_SAMPLES {
            let t = p.t0 + (i as f64 + 0.5) / VIS_SAMPLES as f64 * (p.t1 - p.t0);
            let x = s.curve.point(t);
            if x.distance(a) < near || x.distance(b) < near {
                continue;
            }
            r.sampled += 1;
            let h = 1e-6 * (p.t1 - p.t0).abs().max(1e-12);
            let tan = (s.curve.point(t + h) - s.curve.point(t - h)).normalize_or_zero();
            let mut verdict = Judged::Undecided;
            for (li, &c) in chords.iter().enumerate() {
                let j = judge(level(li), 4.0 * c, near, x, tan, lib);
                match j {
                    Judged::Agree | Judged::Tolerated => {
                        verdict = j;
                        break;
                    }
                    Judged::Differs(_) => verdict = j,
                    Judged::Undecided => {}
                }
                if li + 1 < chords.len() {
                    if j == Judged::Undecided {
                        r.refined_undecided += 1;
                    } else {
                        r.refined_differs += 1;
                    }
                }
            }
            match verdict {
                Judged::Agree => {}
                Judged::Tolerated => r.tolerated += 1,
                Judged::Undecided => r.undecided += 1,
                Judged::Differs(want) => {
                    r.vis_diffs.push((r.errors.len(), x, lib));
                    r.errors.push(format!(
                    "(e) Stück {k} (Strich {}, {:?}) bei t = {t} ({:.4}, {:.4}, {:.4}): Bibliothek {:?}, Referenz {want:?}",
                    p.stroke, s.origin, x.x, x.y, x.z, p.vis
                    ));
                }
            }
        }
    }
    r
}

/// Prüft alle Ansichten aller Fälle gegen die Netzreferenz: keine Fehler, je Ansicht höchstens
/// 30 % und insgesamt höchstens 5 % unentschiedene Referenzstellen. Gibt die Raten aus und
/// liefert `(unentschieden, Stellen)` insgesamt.
pub fn check_all_views(cases: &[(&str, Vec<&Solid>)]) -> (usize, usize) {
    let (mut tolerated, mut sampled, mut warnings, mut undecided) = (0, 0, 0, 0);
    for (name, solids) in cases {
        for k in ViewKind::ALL {
            let r = check_view(solids, k);
            assert!(r.errors.is_empty(), "{name} {k:?}: {:?}", r.errors);
            assert!(
                r.undecided * 10 <= r.sampled * 3,
                "{name} {k:?}: Referenz unentschieden an {} von {} Stellen",
                r.undecided,
                r.sampled
            );
            eprintln!(
                "{name} {k:?}: unentschieden {}/{} ({:.1} %)",
                r.undecided,
                r.sampled,
                100.0 * r.undecided as f64 / r.sampled.max(1) as f64
            );
            tolerated += r.tolerated;
            sampled += r.sampled;
            warnings += r.warnings.len();
            undecided += r.undecided;
        }
    }
    eprintln!(
        "Stellen: {sampled}, zulässig abweichend: {tolerated}, Referenz unentschieden: {undecided}, Warnungen (b): {warnings}"
    );
    assert!(
        undecided * 20 <= sampled,
        "Referenz unentschieden an {undecided} von {sampled} Stellen"
    );
    (undecided, sampled)
}

/// Stellen je Stück in (e).
const VIS_SAMPLES: usize = 24;
/// Sehnentoleranzen der feineren Netze (relativ zu `E`).
const FINER_CHORD_REL: [f64; 2] = [1e-4, 1e-5];

/// Urteil der Referenz an einer Stelle gegenüber der Bibliothek.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Judged {
    Agree,
    /// Abweichung, aber die Referenz wechselt selbst um `near` längs der Linie.
    Tolerated,
    Undecided,
    Differs(bool),
}
/// Nächster Punkt einer Merkmalskante des Netzes im Raum, wenn höchstens `near` entfernt
/// (die Referenz prüft die Netzfassung der Linie: sonst läge der exakte Linienpunkt an hohlen
/// Rundungen, etwa am Bohrungsrand, um bis zu eine Sehnentoleranz im Material des Netzes).
fn snap(scene: &RefScene, near: f64, x: DVec3) -> DVec3 {
    let mut best = (near, x);
    for &(a, b) in scene.features() {
        let (d, q) = seg_dist3(x, a, b);
        if d <= best.0 {
            best = (d, q);
        }
    }
    best.1
}

/// Referenzurteil bei `x` mit der Toleranzregel (e): Eine Abweichung ist zulässig, wenn die
/// Referenz an `x ± near·tan` (Strichtangente) ein **anderes entschiedenes** Urteil hat;
/// unentschiedene Nachbarn zählen nicht als Wechsel. `snap_near`: Einrastabstand für dieses Netz.
fn judge(scene: &RefScene, snap_near: f64, near: f64, x: DVec3, tan: DVec3, lib: bool) -> Judged {
    let at = |y: DVec3| scene.ref_visibility(snap(scene, snap_near, y));
    let Some(want) = at(x) else {
        return Judged::Undecided;
    };
    if want == lib {
        return Judged::Agree;
    }
    let changes = [x + tan * near, x - tan * near]
        .iter()
        .any(|&y| at(y).is_some_and(|v| v != want));
    if changes {
        Judged::Tolerated
    } else {
        Judged::Differs(want)
    }
}
