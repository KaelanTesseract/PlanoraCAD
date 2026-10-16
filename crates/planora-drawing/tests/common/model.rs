//! Prüfkörper mit bekannten Maßen und eigene Messungen (ohne Code aus `planora-drawing`).

use planora_drawing::{basis::Basis, stroke::Stroke};
use planora_kernel::primitives::{make_box, make_cone, make_cylinder, make_sphere};
use planora_kernel::{
    BlendEdge, BlendKind, BlendRequest, Curve, Edge, Face, HalfEdge, Id, Loop, Profile,
    ProfileLoop, Segment, Solid, Surface, UvRect, Vertex, blend, extrude, revolve, validate,
};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};
use std::sync::Arc;

fn tol() -> Tolerance {
    Tolerance::default()
}

/// Quader `[0,a] × [0,b] × [0,c]` (Ecke im Ursprung).
pub fn box_solid(a: f64, b: f64, c: f64) -> Solid {
    make_box(DVec3::new(a, b, c), &tol()).unwrap()
}

/// Zylinder, Achse Z, Mittelpunkt des Bodens im Ursprung.
pub fn cylinder_solid(r: f64, h: f64) -> Solid {
    make_cylinder(r, h, &tol()).unwrap()
}

/// Kugel um den Ursprung.
pub fn sphere_solid(r: f64) -> Solid {
    make_sphere(r, &tol()).unwrap()
}

/// Kegelstumpf, Achse Z, Boden im Ursprung: Bodenradius `r0`, Deckelradius `r1`, Höhe `h`
/// (`r1 == 0`: spitzer Kegel).
pub fn cone_solid(r0: f64, r1: f64, h: f64) -> Solid {
    if r1 == 0.0 {
        return make_cone(r0, h, &tol()).unwrap();
    }
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(r0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(r1, h),
            },
            Segment::Line {
                to: DVec2::new(0.0, h),
            },
        ],
    };
    revolve(&chain, &Frame::WORLD, &tol()).unwrap()
}

/// Normale (nicht normiert) und Höhe der Schnittebene des schräg abgeschnittenen Zylinders:
/// `z = 8 + 0,5·(x·cos 17° + y·sin 17°)`, als `n·p = c` mit `n = (−0,5·cos 17°, −0,5·sin 17°, 1)`,
/// `c = 8`.
pub fn slanted_cut_plane() -> (DVec3, f64) {
    let a = 17f64.to_radians();
    (DVec3::new(-0.5 * a.cos(), -0.5 * a.sin(), 1.0), 8.0)
}

/// Zylinder (Radius `r`, Höhe `h`, Achse Z, Boden im Ursprung), oben schräg abgeschnitten durch
/// die Ebene aus [`slanted_cut_plane`]: Differenz mit einem großen, gedrehten Quader, dessen
/// Unterseite in dieser Ebene liegt.
pub fn slanted_cylinder_solid(r: f64, h: f64) -> Solid {
    use planora_kernel::BoolOp;
    use planora_kernel::boolean::boolean;
    use planora_math::{DMat4, DQuat};
    let (n, c) = slanted_cut_plane();
    let l = 40.0;
    let cutter = make_box(DVec3::splat(l), &tol()).unwrap();
    let rot = DQuat::from_rotation_arc(DVec3::Z, n.normalize());
    let m = DMat4::from_translation(DVec3::new(0.0, 0.0, c))
        * DMat4::from_quat(rot)
        * DMat4::from_translation(DVec3::new(-l / 2.0, -l / 2.0, 0.0));
    let cutter = cutter.transformed(&m).unwrap();
    let out = boolean(&cylinder_solid(r, h), &cutter, BoolOp::Difference, &tol()).unwrap();
    validate(&out).expect("schräg abgeschnittener Zylinder ist gültig");
    out
}

/// Zylinder (Radius `r`, Höhe `h`), dessen Mantel als Kegelfläche mit Steigung 0 beschrieben
/// ist (geometrisch derselbe Körper; Sonderfall für den Umriss).
pub fn flat_slope_cone_solid(r: f64, h: f64) -> Solid {
    let mut s = cylinder_solid(r, h);
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .expect("Zylinderfläche");
    let Surface::Cylinder { frame, radius } = s.faces[id].surface else {
        unreachable!()
    };
    s.faces[id].surface = Surface::Cone {
        frame,
        radius,
        slope: 0.0,
    };
    validate(&s).expect("Kegel mit Steigung 0 ist gültig");
    s
}

/// Untere Halbkugel um den Ursprung (z ≤ 0): Viertelkreis von (0, −r) nach (r, 0), dann zurück
/// zur Achse; Deckfläche ist die Kreisscheibe in z = 0.
pub fn lower_hemisphere_solid(r: f64) -> Solid {
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -r),
        segments: vec![
            Segment::Arc {
                center: DVec2::ZERO,
                sweep: std::f64::consts::FRAC_PI_2,
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    revolve(&chain, &Frame::WORLD, &tol()).unwrap()
}

/// Vollring (eine Torusfläche, zwei Nähte), Achse Z, Mittelpunkt im Ursprung.
pub fn torus_solid(major: f64, minor: f64) -> Solid {
    try_torus_solid(major, minor).expect("Torus ist gültig")
}

/// Wie [`torus_solid`]; `None`, wenn der Kernel den Körper nicht als gültig ansieht (etwa Horn-
/// oder Spindeltorus, `major ≤ minor`).
pub fn try_torus_solid(major: f64, minor: f64) -> Option<Solid> {
    let mut s = Solid::new();
    let v = s.add_vertex(DVec3::new(major + minor, 0.0, 0.0));
    // Meridiankreis bei u = 0: Punkt(t) = Torus(u = 0, v = t).
    let meridian = Frame {
        origin: DVec3::new(major, 0.0, 0.0),
        x: DVec3::X,
        y: DVec3::Z,
        z: -DVec3::Y,
    };
    let e_v = s.add_edge(
        Curve::Circle {
            frame: meridian,
            radius: minor,
        },
        0.0,
        TAU,
        v,
        v,
    );
    // Äußerer Parallelkreis bei v = 0: Punkt(t) = Torus(u = t, v = 0).
    let e_u = s.add_edge(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: major + minor,
        },
        0.0,
        TAU,
        v,
        v,
    );
    let he = |edge: Id<Edge>, forward: bool| HalfEdge { edge, forward };
    s.add_face(Face {
        surface: Surface::Torus {
            frame: Frame::WORLD,
            major,
            minor,
        },
        same_sense: true,
        outer: Loop {
            half_edges: vec![he(e_u, true), he(e_v, true), he(e_u, false), he(e_v, false)],
        },
        inner: vec![],
        patch: Some(UvRect {
            u0: 0.0,
            u1: TAU,
            v0: 0.0,
            v1: TAU,
        }),
        tag: None,
    });
    s.seal();
    validate(&s).ok()?;
    Some(s)
}

/// Umrisspunkte des Vollrings aus [`torus_solid`] (Achse Z, Mitte im Ursprung) für die
/// Blickrichtung `d`, je Zweig ein Streckenzug mit `n` Abschnitten. Eigene Herleitung über die
/// implizite Gleichung `F = (ρ − R)² + z² − r²`: Der Umriss ist `∇F · d = 0`, also
/// `(ρ − R)·a + z·d_z = 0` mit `a = d_x cos u + d_y sin u`; mit `(ρ − R, z) = r·(c, s)` folgt
/// `(c, s) = ±(d_z, −a) / √(a² + d_z²)` (zwei Zweige). Blick in der Ringebene (`|d_z| ≤ 1e-9`):
/// `c·a = 0`, also die Kreise `c = 0` (`z = ±r`) und die Meridiankreise bei `a = 0`.
pub fn torus_silhouette_points(major: f64, minor: f64, d: DVec3, n: usize) -> Vec<Vec<DVec3>> {
    let pt = |u: f64, c: f64, s: f64| {
        let rho = major + minor * c;
        DVec3::new(rho * u.cos(), rho * u.sin(), minor * s)
    };
    let at = |k: usize| TAU * k as f64 / n as f64;
    let mut out = Vec::new();
    if d.z.abs() > 1e-9 {
        for sign in [1.0, -1.0] {
            out.push(
                (0..=n)
                    .map(|k| {
                        let u = at(k);
                        let a = d.x * u.cos() + d.y * u.sin();
                        let h = a.hypot(d.z);
                        pt(u, sign * d.z / h, -sign * a / h)
                    })
                    .collect(),
            );
        }
    } else {
        for s in [1.0, -1.0] {
            out.push((0..=n).map(|k| pt(at(k), 0.0, s)).collect());
        }
        let phi = d.y.atan2(d.x);
        for um in [
            phi + std::f64::consts::FRAC_PI_2,
            phi - std::f64::consts::FRAC_PI_2,
        ] {
            out.push((0..=n).map(|k| pt(um, at(k).cos(), at(k).sin())).collect());
        }
    }
    out
}

/// Zylinder (r = 10, Höhe 7) mit gerundeter Oberkante: Viertelkreis um (7, 7) im (r, z)-Schnitt
/// bis (7, 10), oben eine Scheibe r = 7; Drehkörper um Z, Naht bei 20°. Die Rundung ist eine
/// **Teil**-Torusfläche (R = 7, r = 3, Mitte z = 7, v ∈ [0, π/2]). Liefert den Körper und den
/// Index der Torusfläche.
pub fn rounded_cylinder_solid() -> (Solid, usize) {
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(10.0, 7.0),
            },
            Segment::Arc {
                center: DVec2::new(7.0, 7.0),
                sweep: std::f64::consts::FRAC_PI_2,
            },
            Segment::Line {
                to: DVec2::new(0.0, 10.0),
            },
        ],
    };
    let s = revolve(&chain, &turned_frame(20.0), &tol()).unwrap();
    validate(&s).expect("gerundeter Zylinder ist gültig");
    let face = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        .map(|(id, _)| id.index())
        .expect("Torusfläche");
    (s, face)
}

/// Ein Umrisszweig als Abbildung Parameter ↦ (u, v) auf dem Torus.
type Branch = Box<dyn Fn(f64) -> (f64, f64)>;

/// Umriss des Teil-Torus aus [`rounded_cylinder_solid`] (Achse Z, R, r, Mitte auf Höhe `zc`),
/// beschränkt auf `v ∈ [v0, v1]` (eigene Formel wie [`torus_silhouette_points`], Grenzpunkte per
/// Bisektion in `u` bzw. im Meridianwinkel genau auf `v0`/`v1`). Je zusammenhängendes Stück ein
/// Streckenzug mit Stützstellen im Abstand `TAU / n`.
#[allow(clippy::too_many_arguments)]
pub fn partial_torus_silhouette_points(
    major: f64,
    minor: f64,
    zc: f64,
    d: DVec3,
    n: usize,
    (v0, v1): (f64, f64),
) -> Vec<Vec<DVec3>> {
    let pt = |u: f64, v: f64| {
        let rho = major + minor * v.cos();
        DVec3::new(rho * u.cos(), rho * u.sin(), zc + minor * v.sin())
    };
    let norm = |v: f64| {
        let x = v.rem_euclid(TAU);
        if x > std::f64::consts::PI { x - TAU } else { x }
    };
    let inside = |v: f64| {
        let v = norm(v);
        v >= v0 - 1e-12 && v <= v1 + 1e-12
    };
    // Zweige als (Parameter w ↦ (u, v)) über w ∈ [0, 2π].
    let mut branches: Vec<Branch> = Vec::new();
    if d.z.abs() > 1e-9 {
        for sign in [1.0, -1.0] {
            branches.push(Box::new(move |u: f64| {
                let a = d.x * u.cos() + d.y * u.sin();
                (u, (-sign * a).atan2(sign * d.z))
            }));
        }
    } else {
        for s in [1.0, -1.0] {
            branches.push(Box::new(move |u: f64| (u, s * std::f64::consts::FRAC_PI_2)));
        }
        let phi = d.y.atan2(d.x);
        for um in [
            phi + std::f64::consts::FRAC_PI_2,
            phi - std::f64::consts::FRAC_PI_2,
        ] {
            branches.push(Box::new(move |w: f64| (um, w)));
        }
    }
    let mut out = Vec::new();
    for b in &branches {
        let at = |k: usize| TAU * k as f64 / n as f64;
        let mut run: Vec<DVec3> = Vec::new();
        for k in 0..=n {
            let (u, v) = b(at(k));
            let ok = inside(v);
            if k > 0 {
                let (_, vp) = b(at(k - 1));
                if inside(vp) != ok {
                    // Grenze zwischen at(k−1) und at(k) per Bisektion.
                    let (mut lo, mut hi) = (at(k - 1), at(k));
                    let lo_in = inside(vp);
                    for _ in 0..80 {
                        let m = 0.5 * (lo + hi);
                        if inside(b(m).1) == lo_in {
                            lo = m;
                        } else {
                            hi = m;
                        }
                    }
                    let w = if lo_in { lo } else { hi };
                    let (bu, bv) = b(w);
                    let bv = if (norm(bv) - v0).abs() < (norm(bv) - v1).abs() {
                        v0
                    } else {
                        v1
                    };
                    run.push(pt(bu, bv));
                    if lo_in {
                        out.push(std::mem::take(&mut run));
                    }
                }
            }
            if ok {
                run.push(pt(u, v));
            }
        }
        if run.len() > 1 {
            out.push(run);
        }
    }
    out.retain(|r| r.len() > 1);
    out
}

/// `n + 1` gleichverteilte Punkte des Strichs, eigene Projektion `(P·ex, P·ey)`.
pub fn sample2(s: &Stroke, basis: &Basis, n: usize) -> Vec<DVec2> {
    sample3(s, n)
        .into_iter()
        .map(|p| DVec2::new(p.dot(basis.ex), p.dot(basis.ey)))
        .collect()
}

/// `n + 1` gleichverteilte Raumpunkte des Strichs.
pub fn sample3(s: &Stroke, n: usize) -> Vec<DVec3> {
    (0..=n)
        .map(|i| s.curve.point(s.t0 + (s.t1 - s.t0) * i as f64 / n as f64))
        .collect()
}

/// Abstand des Punkts `p` von der Strecke `a b` (2D).
fn seg_dist2(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 > 0.0 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// Gerichteter Abstand: größter Abstand eines Punkts der Streckenzüge `a` vom nächsten
/// Streckenabschnitt der Streckenzüge `b` (Raster über die Abschnitte von `b`).
fn directed(a: &[Vec<DVec2>], b: &[Vec<DVec2>]) -> f64 {
    let segs: Vec<(DVec2, DVec2)> = b
        .iter()
        .flat_map(|l| l.windows(2).map(|w| (w[0], w[1])))
        .collect();
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for (p, q) in &segs {
        lo = lo.min(p.min(*q));
        hi = hi.max(p.max(*q));
    }
    const G: usize = 256;
    let cell = ((hi - lo) / G as f64).max(DVec2::splat(1e-9));
    let idx = |p: DVec2| {
        let q = (p - lo) / cell;
        (
            (q.x.floor().max(0.0) as usize).min(G - 1),
            (q.y.floor().max(0.0) as usize).min(G - 1),
        )
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); G * G];
    for (k, (p, q)) in segs.iter().enumerate() {
        let (i0, j0) = idx(p.min(*q));
        let (i1, j1) = idx(p.max(*q));
        for i in i0..=i1 {
            for j in j0..=j1 {
                grid[i * G + j].push(k);
            }
        }
    }
    let mut worst: f64 = 0.0;
    for p in a.iter().flatten() {
        // Ringweise wachsende Suche, bis der Ring weiter weg ist als der beste Abstand.
        let (ci, cj) = idx(*p);
        let mut best = f64::INFINITY;
        for ring in 0..G {
            if ring > 0 && (ring - 1) as f64 * cell.x.min(cell.y) > best {
                break;
            }
            let (i0, i1) = (ci.saturating_sub(ring), (ci + ring).min(G - 1));
            let (j0, j1) = (cj.saturating_sub(ring), (cj + ring).min(G - 1));
            for i in i0..=i1 {
                for j in j0..=j1 {
                    if i != i0 && i != i1 && j != j0 && j != j1 {
                        continue;
                    }
                    for &k in &grid[i * G + j] {
                        best = best.min(seg_dist2(*p, segs[k].0, segs[k].1));
                    }
                }
            }
        }
        worst = worst.max(best);
    }
    worst
}

/// Symmetrischer Hausdorff-Abstand zweier Mengen von Streckenzügen (Punkt gegen Abschnitt).
pub fn hausdorff(a: &[Vec<DVec2>], b: &[Vec<DVec2>]) -> f64 {
    directed(a, b).max(directed(b, a))
}

/// Eigene Rechnung zur Freiformfläche `face` des Körpers: nächster Flächenpunkt zu `p` – erst
/// global (Raster 64 × 64 über den Parameterbereich), dann örtlich von den vier besten
/// Rasterpunkten aus Newton auf `(S − p)·S_u = (S − p)·S_v = 0` mit Schrittverkürzung (ein
/// Schritt zählt nur, wenn der Abstand sinkt; am Bereichsrand begrenzt), zuletzt ein feines
/// Raster um den besten Punkt. Der kleinste je gefundene Abstand gilt. Dort `n·d` mit der eigenen
/// Normale `S_u × S_v / |S_u × S_v|`. Liefert `(Abstand, n·d)`.
pub fn surface_residual(s: &Solid, face: usize, p: DVec3, d: DVec3) -> (f64, f64) {
    let Surface::Nurbs(data) = &s.faces[Id::<Face>::from_index(face)].surface else {
        panic!("keine Freiformfläche");
    };
    let sf = data.surface();
    let ((u0, u1), (v0, v1)) = sf.domain();
    let dist = |u: f64, v: f64| sf.point(u, v).distance(p);
    const N: usize = 64;
    let mut grid: Vec<(f64, f64, f64)> = Vec::with_capacity((N + 1) * (N + 1));
    for i in 0..=N {
        for j in 0..=N {
            let u = u0 + (u1 - u0) * i as f64 / N as f64;
            let v = v0 + (v1 - v0) * j as f64 / N as f64;
            grid.push((dist(u, v), u, v));
        }
    }
    grid.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut best = grid[0];
    for &(d0, mut u, mut v) in grid.iter().take(4) {
        let mut cur = d0;
        for _ in 0..60 {
            let dv = sf.derivs(u, v, 2);
            let r = dv[0][0] - p;
            let (su, sv) = (dv[1][0], dv[0][1]);
            let (f, g) = (r.dot(su), r.dot(sv));
            let (a, b, c) = (
                su.dot(su) + r.dot(dv[2][0]),
                su.dot(sv) + r.dot(dv[1][1]),
                sv.dot(sv) + r.dot(dv[0][2]),
            );
            let det = a * c - b * b;
            let (mut du, mut dw) = if det.abs() > 1e-300 && a > 0.0 && det > 0.0 {
                ((c * f - b * g) / det, (a * g - b * f) / det)
            } else {
                // Gradientenschritt, wenn die Hesse-Matrix nicht positiv ist.
                let l = (su.length_squared() + sv.length_squared()).max(1e-300);
                (f / l, g / l)
            };
            let mut moved = false;
            for _ in 0..30 {
                let (nu, nv) = ((u - du).clamp(u0, u1), (v - dw).clamp(v0, v1));
                let nd = dist(nu, nv);
                if nd < cur {
                    (u, v, cur) = (nu, nv, nd);
                    moved = true;
                    break;
                }
                du *= 0.5;
                dw *= 0.5;
            }
            if !moved {
                break;
            }
        }
        if cur < best.0 {
            best = (cur, u, v);
        }
    }
    // Feines Raster um den besten Punkt (eine Rasterzelle zu jeder Seite).
    let (hu, hv) = ((u1 - u0) / N as f64, (v1 - v0) / N as f64);
    let (bu, bv) = (best.1, best.2);
    for i in 0..=40 {
        for j in 0..=40 {
            let u = (bu - hu + 2.0 * hu * i as f64 / 40.0).clamp(u0, u1);
            let v = (bv - hv + 2.0 * hv * j as f64 / 40.0).clamp(v0, v1);
            let dd = dist(u, v);
            if dd < best.0 {
                best = (dd, u, v);
            }
        }
    }
    let dv = sf.derivs(best.1, best.2, 1);
    let n = dv[1][0].cross(dv[0][1]).normalize();
    (best.0, n.dot(d))
}

/// Eigenes Sichturteil für eine ungekippte, unbeschnittene Kuppel aus [`dome_revolved`] (Sockel
/// r = 8 bis z = 4, Wand mit den (r, z)-Kontrollpunkten `wall`, z streng steigend, oben eine
/// Scheibe bis zur Achse): Der Körper ist `{(ρ, z) : 0 ≤ z ≤ z_top, ρ ≤ R(z)}` mit dem eigenen
/// Profilradius `R` (Tabelle aus der Bernstein-Form, 100 000 Stellen, linear interpoliert).
/// Prüfstrahlen von `p` zum Auge, um `1e-4` in vier Richtungen gekippt (eigener Wert); längs eines
/// Strahls wird `f = max(ρ − R(z), −z, z − z_top)` (< 0 innen) ab `s0 = 0,1` alle 2e-3 abgetastet
/// und um jedes örtliche Minimum per Goldenem Schnitt verfeinert. Frei: kleinstes `f > 1e-6`;
/// verdeckt: `f < −1e-6`. Sichtbar, wenn ein Strahl frei ist; verdeckt, wenn alle verdeckt sind;
/// sonst `None` (unentschieden). Strahlen erst ab `s0`: Die Linienpunkte (auch angepasste Umrisse,
/// bis 1e-4 neben der Fläche) liegen auf oder an der Fläche.
pub struct DomeJudge {
    wall: [(f64, f64); 4],
    z_lo: f64,
    z_top: f64,
    table: Vec<f64>,
}

impl DomeJudge {
    pub fn new(wall: [(f64, f64); 4]) -> DomeJudge {
        let bern = |t: f64, k: usize| {
            let s = 1.0 - t;
            let w = [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t];
            w.iter()
                .zip(&wall)
                .map(|(b, c)| b * if k == 0 { c.0 } else { c.1 })
                .sum::<f64>()
        };
        let (z_lo, z_top) = (wall[0].1, wall[3].1);
        const M: usize = 100_000;
        let table = (0..=M)
            .map(|i| {
                let z = z_lo + (z_top - z_lo) * i as f64 / M as f64;
                let (mut a, mut b) = (0.0, 1.0);
                for _ in 0..60 {
                    let m = 0.5 * (a + b);
                    if bern(m, 1) < z {
                        a = m;
                    } else {
                        b = m;
                    }
                }
                bern(0.5 * (a + b), 0)
            })
            .collect();
        DomeJudge {
            wall,
            z_lo,
            z_top,
            table,
        }
    }

    fn radius(&self, z: f64) -> f64 {
        if z <= self.z_lo {
            return self.wall[0].0;
        }
        let m = self.table.len() - 1;
        let x = ((z - self.z_lo) / (self.z_top - self.z_lo) * m as f64).clamp(0.0, m as f64);
        let i = (x.floor() as usize).min(m - 1);
        let f = x - i as f64;
        self.table[i] * (1.0 - f) + self.table[i + 1] * f
    }

    fn f(&self, q: DVec3) -> f64 {
        let rho = q.x.hypot(q.y);
        (rho - self.radius(q.z.clamp(0.0, self.z_top)))
            .max(-q.z)
            .max(q.z - self.z_top)
    }

    /// Kleinstes `f` längs `p + s·dir`, `s ∈ [s0, Austritt aus der Hüllbox]`.
    fn ray_min(&self, p: DVec3, dir: DVec3) -> f64 {
        const S0: f64 = 0.1;
        const H: f64 = 2e-3;
        let lo = DVec3::new(-12.0, -12.0, -2.0);
        let hi = DVec3::new(12.0, 12.0, self.z_top + 2.0);
        let mut s_end = f64::INFINITY;
        for k in 0..3 {
            if dir[k] != 0.0 {
                let a = (lo[k] - p[k]) / dir[k];
                let b = (hi[k] - p[k]) / dir[k];
                s_end = s_end.min(a.max(b));
            }
        }
        let n = ((s_end - S0) / H).ceil().max(1.0) as usize;
        let at = |i: usize| S0 + (s_end - S0) * i as f64 / n as f64;
        let vals: Vec<f64> = (0..=n).map(|i| self.f(p + dir * at(i))).collect();
        let mut best = vals.iter().copied().fold(f64::INFINITY, f64::min);
        for i in 1..n {
            if vals[i] <= vals[i - 1] && vals[i] <= vals[i + 1] {
                let (mut a, mut b) = (at(i - 1), at(i + 1));
                let g = 0.5 * (5f64.sqrt() - 1.0);
                for _ in 0..60 {
                    let (x1, x2) = (b - g * (b - a), a + g * (b - a));
                    if self.f(p + dir * x1) <= self.f(p + dir * x2) {
                        b = x2;
                    } else {
                        a = x1;
                    }
                }
                best = best.min(self.f(p + dir * (0.5 * (a + b))));
            }
        }
        best
    }

    /// Sichturteil an `p` für die Ansicht `basis` (siehe Typbeschreibung).
    pub fn visible(&self, p: DVec3, basis: &Basis) -> Option<bool> {
        const TILT: f64 = 1e-4;
        const MARGIN: f64 = 1e-6;
        let eye = -basis.d;
        let mut all_blocked = true;
        for t in [basis.ex, -basis.ex, basis.ey, -basis.ey] {
            let m = self.ray_min(p, (eye + t * TILT).normalize());
            if m > MARGIN {
                return Some(true);
            }
            if m >= -MARGIN {
                all_blocked = false;
            }
        }
        all_blocked.then_some(false)
    }
}
/// Rahmen um Z gedreht (Winkel in Grad): die Naht der Drehflächen liegt dann bei diesem Winkel,
/// nicht auf den Umrissen der Normalansichten.
fn turned_frame(deg: f64) -> Frame {
    let a = deg.to_radians();
    Frame {
        origin: DVec3::ZERO,
        x: DVec3::new(a.cos(), a.sin(), 0.0),
        y: DVec3::new(-a.sin(), a.cos(), 0.0),
        z: DVec3::Z,
    }
}

/// Kuppel als Drehkörper (Achse Z, Naht bei 20°): Sockel (Zylinder r = 8, Höhe 4), darauf eine
/// Freiformwand aus der kubischen Bézierkurve mit den (r, z)-Kontrollpunkten `ctrl` (Anfang
/// (8, 4)), oben eine Kreisscheibe bis zur Achse.
fn dome_revolved(ctrl: [(f64, f64); 4]) -> Solid {
    let bezier = cubic_from_controls(&ctrl.map(|(r, z)| (r, z, 0.0)));
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(8.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(8.0, 4.0),
            },
            Segment::Nurbs {
                curve: Arc::new(bezier),
            },
            Segment::Line {
                to: DVec2::new(0.0, ctrl[3].1),
            },
        ],
    };
    revolve(&chain, &turned_frame(20.0), &tol()).unwrap()
}

/// Normale (nicht normiert) und Wert der Schnittebene der Kuppel: `x + 0,35·y + 0,25·z = 8`.
pub fn dome_cut_plane() -> (DVec3, f64) {
    (DVec3::new(1.0, 0.35, 0.25), 8.0)
}

/// Der Körper ohne den Halbraum jenseits der Ebene [`dome_cut_plane`] (Differenz mit einem
/// großen Quader, dessen Unterseite in der Ebene liegt), geprüft; mit dem Index der
/// Freiformfläche.
fn cut_by_plane(s: &Solid) -> (Solid, usize) {
    use planora_kernel::BoolOp;
    use planora_kernel::boolean::boolean;
    use planora_math::{DMat4, DQuat};
    let (n, c) = dome_cut_plane();
    let nn = n.normalize();
    let l = 60.0;
    let cutter = make_box(DVec3::splat(l), &tol()).unwrap();
    let rot = DQuat::from_rotation_arc(DVec3::Z, nn);
    let m = DMat4::from_translation(nn * (c / n.length()))
        * DMat4::from_quat(rot)
        * DMat4::from_translation(DVec3::new(-l / 2.0, -l / 2.0, 0.0));
    let cutter = cutter.transformed(&m).unwrap();
    let out = boolean(s, &cutter, BoolOp::Difference, &tol()).unwrap();
    validate(&out).expect("abgeschnittene Kuppel ist gültig");
    let face = out
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id.index())
        .expect("Freiformfläche");
    (out, face)
}

/// Kuppel aus [`dome_revolved`] mit der Wand (8, 4) → (6,5; 7) → (4,5; 8,5) → (3; 9,5) (z steigt,
/// r fällt streng: kein Überhang; alle Kanten scharf mit Knick über 20°, wie die Merkmalskanten
/// des Referenznetzes), leicht gekippt und schräg abgeschnitten ([`cut_by_plane`]): Die
/// Freiformfläche ist damit **innerhalb** ihres Parameterbereichs beschnitten. Liefert den Körper
/// und den Index der Freiformfläche.
pub fn nurbs_dome_solid() -> (Solid, usize) {
    // Gekippt (9° um (1; 0,6; 0)), damit kein Kreis in einer Normalansicht genau von der Seite
    // erscheint (deckungsgleiche Kreishälften, deren Sichtbarkeit an Materialtiefen unter der
    // Sehnentoleranz des Referenznetzes hinge).
    let dome = dome_revolved([(8.0, 4.0), (6.5, 7.0), (4.5, 8.5), (3.0, 9.5)]);
    cut_by_plane(&rotated(
        &dome,
        DVec3::new(1.0, 0.6, 0.0),
        9f64.to_radians(),
    ))
}

/// Kontrollpunkte (r, z) der tangential ansetzenden Kuppelwand: (8, 4) → (8, 6) → (6, 9) →
/// (3; 9,5) – die Wand setzt senkrecht (tangential) auf den Sockelzylinder auf.
pub const TANGENT_DOME_WALL: [(f64, f64); 4] = [(8.0, 4.0), (8.0, 6.0), (6.0, 9.0), (3.0, 9.5)];

/// Kuppel aus [`dome_revolved`] mit tangential ansetzender Wand ([`TANGENT_DOME_WALL`]),
/// ungekippt und unbeschnitten (der häufigste Freiformfall: Spline-Profil glatt auf einem
/// Zylinder). Liefert den Körper und den Index der Freiformfläche.
pub fn nurbs_tangent_dome_solid() -> (Solid, usize) {
    let s = dome_revolved(TANGENT_DOME_WALL);
    validate(&s).expect("tangentiale Kuppel ist gültig");
    let face = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id.index())
        .expect("Freiformfläche");
    (s, face)
}

/// Kuppel mit Überhang: Wand (8, 4) → (11, 5) → (7; 8,5) → (3, 9) (r wächst erst bis ≈ 9, die
/// Wand ragt über den Sockel), ungekippt, schräg abgeschnitten wie [`nurbs_dome_solid`]. Von
/// hinten endet ein Umriss der Wand auf der Schnittkante an einer Stelle, an der jeder Prüfstrahl
/// den Rand der Schnittfläche streift.
pub fn nurbs_overhang_dome_solid() -> (Solid, usize) {
    cut_by_plane(&dome_revolved([
        (8.0, 4.0),
        (11.0, 5.0),
        (7.0, 8.5),
        (3.0, 9.0),
    ]))
}
/// Kuppel mit Pol: Sockel (Zylinder r = 8, Höhe 4), darauf eine Freiformwand aus der kubischen
/// Bézierkurve (8, 4) → (10, 6) → (6, 10) → (0, 10), die auf der Achse endet (Pol oben).
pub fn nurbs_pole_solid() -> Solid {
    let bezier = cubic_from_controls(&[
        (8.0, 4.0, 0.0),
        (10.0, 6.0, 0.0),
        (6.0, 10.0, 0.0),
        (0.0, 10.0, 0.0),
    ]);
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(8.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(8.0, 4.0),
            },
            Segment::Nurbs {
                curve: Arc::new(bezier),
            },
        ],
    };
    let s = revolve(&chain, &turned_frame(20.0), &tol()).unwrap();
    validate(&s).expect("Kuppel mit Pol ist gültig");
    s
}

/// Halbzylinder: Halbkreis (Radius `r`, Mittelpunkt im Ursprung, y ≥ 0) mal Höhe `h` in Z.
pub fn half_cylinder_solid(r: f64, h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::new(-r, 0.0),
            segments: vec![
                Segment::Line {
                    to: DVec2::new(r, 0.0),
                },
                Segment::Arc {
                    center: DVec2::ZERO,
                    sweep: std::f64::consts::PI,
                },
            ],
        },
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, h, &tol()).unwrap()
}

/// Quadratische Platte `[−half, half]² × [0, h]` mit einem durchgehenden Rundloch (Radius `r`,
/// Achse Z durch den Ursprung).
pub fn plate_with_hole_solid(half: f64, r: f64, h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::splat(-half), DVec2::splat(half)),
        holes: vec![ProfileLoop::circle(DVec2::ZERO, r)],
    }
    .try_oriented()
    .unwrap();
    extrude(&profile, &Frame::WORLD, h, &tol()).unwrap()
}

/// Platte 10 × 10 × 5, deren Kante bei x = 10 durch eine sanft gewölbte kubische Bézierkurve
/// mit den Kontrollpunkten (10,0), (10,43; 3,33), (10,43; 6,67), (10,10) ersetzt ist (eine
/// Freiformwand). Bei y = 5 (t = ½) liegt der Rand bei x = 10/8 + 10,43·6/8 + 10/8 = 10,3225.
pub fn bulged_plate_solid() -> Solid {
    let bezier = cubic_from_controls(&[
        (10.0, 0.0, 0.0),
        (10.43, 3.33, 0.0),
        (10.43, 6.67, 0.0),
        (10.0, 10.0, 0.0),
    ]);
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(bezier),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, 5.0, &tol()).unwrap()
}

/// Platte mit einer Freiformwand: Grundriss (0,0) → (10,0) → Spline durch (10,0), (12,5), (10,10)
/// → (0,10) → (0,0), Höhe 5. Liefert den Körper und den Index der Freiformfläche.
pub fn nurbs_slab_solid() -> (Solid, usize) {
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(spline),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = extrude(&profile, &Frame::WORLD, 5.0, &tol()).unwrap();
    let face = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id.index())
        .expect("Freiformfläche");
    (s, face)
}

/// Punkt auf der Freiformwand der Platte (Mitte des Splines, halbe Höhe) und ihre Außennormale,
/// aus der Symmetrie des Splines (Spiegelung an y = 5) von Hand bestimmt.
pub fn nurbs_slab_center_point_and_normal() -> (DVec3, DVec3) {
    (DVec3::new(12.0, 5.0, 2.5), DVec3::X)
}

/// Quader mit einer verrundeten Kante (Kante parallel zu Y bei x = a, z = c, Radius `r`).
pub fn box_with_one_rounded_edge(a: f64, b: f64, c: f64, r: f64) -> Solid {
    let s = box_solid(a, b, c);
    let at = |id: Id<Vertex>| s.vertices[id].point;
    let edge = s
        .edges
        .iter()
        .find(|(_, e)| {
            let (p, q) = (at(e.start), at(e.end));
            (p.x - a).abs() < 1e-9
                && (q.x - a).abs() < 1e-9
                && (p.z - c).abs() < 1e-9
                && (q.z - c).abs() < 1e-9
                && (p.y - q.y).abs() > 1e-9
        })
        .map(|(id, _)| id)
        .expect("Kante bei x = a, z = c");
    let req = BlendRequest {
        edges: vec![BlendEdge {
            edge,
            first_face: None,
        }],
        kind: BlendKind::Fillet { radius: r },
        tangent_chain: false,
    };
    let out = blend::blend(&s, &req, &tol()).unwrap();
    validate(&out.solid).expect("gerundeter Quader ist gültig");
    out.solid
}

/// Flächen je Kante (Index in `solid.faces`), je Halbkante einmal.
fn faces_of_edges(s: &Solid) -> Vec<Vec<usize>> {
    let mut v: Vec<Vec<usize>> = vec![vec![]; s.edges.len()];
    for (fid, f) in s.faces.iter() {
        for lp in std::iter::once(&f.outer).chain(f.inner.iter()) {
            for he in &lp.half_edges {
                v[he.edge.index()].push(fid.index());
            }
        }
    }
    v
}

fn is_seam(faces: &[usize]) -> bool {
    faces.len() >= 2 && faces.iter().all(|&f| f == faces[0])
}

/// Zahl der Nahtkanten (beide Seiten dieselbe Fläche).
pub fn count_seam_edges(s: &Solid) -> usize {
    faces_of_edges(s).iter().filter(|f| is_seam(f)).count()
}

/// Zahl der glatten Kanten zwischen zwei **verschiedenen** Flächen: Winkel der Außennormalen an
/// 5 gleichverteilten Stellen der Kante überall höchstens 0,5°.
pub fn count_smooth_edges(s: &Solid) -> usize {
    let limit = (0.5f64).to_radians().cos();
    let uses = faces_of_edges(s);
    let mut n = 0;
    for (eid, e) in s.edges.iter() {
        let f = &uses[eid.index()];
        if f.len() != 2 || is_seam(f) {
            continue;
        }
        let smooth = (0..5).all(|i| {
            let t = e.t0 + (e.t1 - e.t0) * i as f64 / 4.0;
            let p = e.curve.point(t);
            let normal = |fi: usize| {
                let face = &s.faces[Id::<Face>::from_index(fi)];
                let (u, v) = face.surface.project(p);
                let n = face.surface.normal(u, v);
                if face.same_sense { n } else { -n }
            };
            normal(f[0]).dot(normal(f[1])) >= limit
        });
        if smooth {
            n += 1;
        }
    }
    n
}

/// Gesamtlänge der projizierten Striche: Mittelpunktsregel `∑ |p'(t)| dt` mit 4096 Stützstellen
/// je Strich, `p'` als symmetrischer Differenzenquotient der projizierten Kurvenpunkte.
pub fn total_len(strokes: &[Stroke], basis: &Basis) -> f64 {
    const N: usize = 4096;
    let mut total = 0.0;
    for s in strokes {
        let proj = |t: f64| {
            let p = s.curve.point(t);
            (p.dot(basis.ex), p.dot(basis.ey))
        };
        let dt = (s.t1 - s.t0) / N as f64;
        let h = (s.t1 - s.t0).abs() * 1e-6;
        for i in 0..N {
            let t = s.t0 + dt * (i as f64 + 0.5);
            let (a, b) = (proj(t + h), proj(t - h));
            let d = ((a.0 - b.0) / (2.0 * h)).hypot((a.1 - b.1) / (2.0 * h));
            total += d * dt.abs();
        }
    }
    total
}

/// Ähnlichkeitsabbildung `m` auf einen Körper (öffentliche Kernel-Transformation).
fn mapped(s: &Solid, m: &planora_math::DMat4) -> Solid {
    s.transformed(m).unwrap()
}

/// Boolesche Verknüpfung über die öffentliche Kernel-Schnittstelle, Ergebnis geprüft.
fn boolean_op(a: &Solid, b: &Solid, op: planora_kernel::BoolOp) -> Solid {
    let out = planora_kernel::boolean::boolean(a, b, op, &tol()).unwrap();
    validate(&out).expect("Ergebnis der Booleschen Verknüpfung ist gültig");
    out
}

/// Zylinder (Radius `r`) längs der Achse `axis` (X, Y oder Z) durch den Punkt `center`, der
/// zu beiden Seiten `half` lang ist.
pub fn axis_cylinder(r: f64, half: f64, axis: DVec3, center: DVec3) -> Solid {
    use planora_math::{DMat4, DQuat};
    let c = cylinder_solid(r, 2.0 * half);
    let rot = DQuat::from_rotation_arc(DVec3::Z, axis.normalize());
    let m = DMat4::from_translation(center)
        * DMat4::from_quat(rot)
        * DMat4::from_translation(DVec3::new(0.0, 0.0, -half));
    mapped(&c, &m)
}

/// Quader `[0,a] × [0,b] × [0,c]` mit einer durchgehenden Bohrung (Radius `hole_r`) längs
/// `axis` (X, Y oder Z) durch die Quadermitte.
pub fn box_with_hole(a: f64, b: f64, c: f64, hole_r: f64, axis: DVec3) -> Solid {
    let center = DVec3::new(a, b, c) * 0.5;
    let half = a.max(b).max(c);
    let drill = axis_cylinder(hole_r, half, axis, center);
    boolean_op(
        &box_solid(a, b, c),
        &drill,
        planora_kernel::BoolOp::Difference,
    )
}

/// L-Winkel: Grundriss `(0,0) → (a,0) → (a,t) → (t,t) → (t,b) → (0,b)`, Höhe `c` in Z.
pub fn l_bracket(a: f64, b: f64, c: f64, t: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(a, 0.0),
                },
                Segment::Line {
                    to: DVec2::new(a, t),
                },
                Segment::Line {
                    to: DVec2::new(t, t),
                },
                Segment::Line {
                    to: DVec2::new(t, b),
                },
                Segment::Line {
                    to: DVec2::new(0.0, b),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, c, &tol()).unwrap()
}

/// Quader `[0,a] × [0,b] × [0,c]` mit einer rechteckigen Tasche (`pw` in X, `pl` in Y, Tiefe
/// `depth`) mittig in der Oberseite.
pub fn box_with_pocket(a: f64, b: f64, c: f64, pw: f64, pl: f64, depth: f64) -> Solid {
    use planora_math::DMat4;
    let cutter = mapped(
        &box_solid(pw, pl, depth + 1.0),
        &DMat4::from_translation(DVec3::new(0.5 * (a - pw), 0.5 * (b - pl), c - depth)),
    );
    boolean_op(
        &box_solid(a, b, c),
        &cutter,
        planora_kernel::BoolOp::Difference,
    )
}

/// Zylinder (Radius `r`, Höhe `h`, Achse Z) mit einem Querdurchbruch längs X auf halber Höhe
/// durch die Achse. Der Kernel schneidet zwei sich kreuzende Zylinder nicht
/// (`UnsupportedIntersection { Zylinder, Zylinder }`); statt der runden Querbohrung hat der
/// Durchbruch daher einen auf die Spitze gestellten quadratischen Querschnitt (halbe Diagonale
/// `hole_r`, Seiten unter 45° zur Achse). Seine Seitenflächen schneiden den Mantel in
/// Ellipsenbögen – gekrümmte Schnittkanten wie bei der Bohrung, mit Ecken.
pub fn cylinder_cross_hole(r: f64, h: f64, hole_r: f64) -> Solid {
    use planora_math::{DMat4, DQuat};
    let side = hole_r * std::f64::consts::SQRT_2;
    let len = 4.0 * r;
    let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 0.5 * h))
        * DMat4::from_quat(DQuat::from_rotation_x(std::f64::consts::FRAC_PI_4))
        * DMat4::from_translation(DVec3::new(-0.5 * len, -0.5 * side, -0.5 * side));
    let drill = mapped(&box_solid(len, side, side), &m);
    boolean_op(
        &cylinder_solid(r, h),
        &drill,
        planora_kernel::BoolOp::Difference,
    )
}

/// Würfel `[0,a]³` und Kugel (Radius `r`) um die Mitte der Oberseite, vereinigt.
pub fn sphere_on_box(a: f64, r: f64) -> Solid {
    use planora_math::DMat4;
    let ball = mapped(
        &sphere_solid(r),
        &DMat4::from_translation(DVec3::new(0.5 * a, 0.5 * a, a)),
    );
    boolean_op(&box_solid(a, a, a), &ball, planora_kernel::BoolOp::Union)
}

/// Quader `[0,a] × [0,b] × [0,c]`, dessen vier senkrechte Kanten (parallel zu Z) mit Radius `r`
/// verrundet sind (nur Zylinderflächen, keine Eckflächen).
pub fn rounded_box(a: f64, b: f64, c: f64, r: f64) -> Solid {
    let s = box_solid(a, b, c);
    let at = |id: Id<Vertex>| s.vertices[id].point;
    let edges: Vec<BlendEdge> = s
        .edges
        .iter()
        .filter(|(_, e)| {
            let (p, q) = (at(e.start), at(e.end));
            (p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9
        })
        .map(|(id, _)| BlendEdge {
            edge: id,
            first_face: None,
        })
        .collect();
    assert_eq!(edges.len(), 4, "vier senkrechte Kanten");
    let req = BlendRequest {
        edges,
        kind: BlendKind::Fillet { radius: r },
        tangent_chain: false,
    };
    let out = blend::blend(&s, &req, &tol()).unwrap();
    validate(&out.solid).expect("verrundeter Quader ist gültig");
    out.solid
}

/// Gleichmäßig um den Faktor `f` skaliert (um den Ursprung), dann um `shift·(1, 1, 1)`
/// verschoben (öffentliche Kernel-Transformation).
pub fn transformed(s: &Solid, f: f64, shift: f64) -> Solid {
    use planora_math::DMat4;
    let m = DMat4::from_translation(DVec3::splat(shift)) * DMat4::from_scale(DVec3::splat(f));
    mapped(s, &m)
}

/// Um `v` verschoben.
pub fn translated(s: &Solid, v: DVec3) -> Solid {
    mapped(s, &planora_math::DMat4::from_translation(v))
}

/// Quader `[lo, hi]` (Ecken in Weltkoordinaten).
pub fn box_at(lo: (f64, f64, f64), hi: (f64, f64, f64)) -> Solid {
    let lo = DVec3::new(lo.0, lo.1, lo.2);
    let hi = DVec3::new(hi.0, hi.1, hi.2);
    let d = hi - lo;
    translated(&box_solid(d.x, d.y, d.z), lo)
}

/// Um die Achse `axis` durch den Ursprung um `angle` (Radiant) gedreht.
pub fn rotated(s: &Solid, axis: DVec3, angle: f64) -> Solid {
    use planora_math::{DMat4, DQuat};
    mapped(
        s,
        &DMat4::from_quat(DQuat::from_axis_angle(axis.normalize(), angle)),
    )
}

/// Gerader Strich von `a` in Richtung `dir` (normiert), Parameter `0 … len` (Länge in mm).
pub fn line_stroke(a: (f64, f64, f64), dir: (f64, f64, f64), len: f64) -> Stroke {
    Stroke {
        curve: Curve::Line {
            origin: DVec3::new(a.0, a.1, a.2),
            dir: DVec3::new(dir.0, dir.1, dir.2).normalize(),
        },
        t0: 0.0,
        t1: len,
        origin: planora_drawing::stroke::StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

/// Strich mit seiner Ansichtsbasis.
pub struct BasisStroke {
    pub basis: Basis,
    pub stroke: Stroke,
}

/// Dünne Platte `[0,10] × [0,1] × [0,10]`, Vorderansicht (Blick in +Y, Auge bei −Y). Dahinter,
/// im Abstand `GAP` hinter der Rückseite (`y = 1 + GAP`, `z = 5`), läuft ein Strich in +X von
/// `x = −3` bis `x = 7`. Links von `x = 0` ist er frei, rechts davon verdeckt die Platte ihn:
/// Übergang bei `t* = 3` (aus der Geometrie: `x(t) = −3 + t = 0`). Der kleine Abstand
/// `GAP = 5e-4` hält die Verschiebung, die gekippte Prüfstrahlen am Plattenrand bewirken können
/// (höchstens Kippwinkel · Abstand ≤ 1e-4 · 5e-4 = 5e-8 bei der größten zulässigen Kippung),
/// unter der Genauigkeit des Tests (1e-7). Szene mit Ausdehnung 100 (wie im Test).
pub fn stroke_behind_plate_edge() -> (BasisStroke, planora_drawing::raycast::Scene, f64) {
    const GAP: f64 = 5e-4;
    let plate = box_solid(10.0, 1.0, 10.0);
    let bodies = [planora_drawing::DrawBody {
        name: "Platte".into(),
        solid: &plate,
    }];
    let scene =
        planora_drawing::raycast::Scene::new(&bodies, 100.0, &planora_exchange::Progress::new())
            .unwrap();
    let stroke = line_stroke((-3.0, 1.0 + GAP, 5.0), (1.0, 0.0, 0.0), 10.0);
    (
        BasisStroke {
            basis: Basis::of(planora_drawing::model::ViewKind::Front),
            stroke,
        },
        scene,
        3.0,
    )
}

/// Kubische Bézierkurve als NURBS (offener Knotenvektor `[0,0,0,0,1,1,1,1]`, Gewichte 1) mit
/// den vier Punkten als Kontrollpunkten.
pub fn cubic_from_controls(p: &[(f64, f64, f64)]) -> NurbsCurve {
    assert_eq!(p.len(), 4, "genau vier Kontrollpunkte");
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        p.iter().map(|&(x, y, z)| DVec3::new(x, y, z)).collect(),
        vec![1.0; 4],
    )
    .unwrap()
}

/// Kubische Bézierkurve (wie [`cubic_from_controls`]), die **durch** die vier Punkte läuft, bei
/// den Parametern 0, 1/3, 2/3, 1. Die inneren Kontrollpunkte von Hand aus den
/// Bernstein-Gleichungen `B(1/3) = Q1`, `B(2/3) = Q2`:
/// `R1 = 27·Q1 − 8·Q0 − Q3 = 12·P1 + 6·P2`, `R2 = 27·Q2 − Q0 − 8·Q3 = 6·P1 + 12·P2`,
/// also `P1 = (2·R1 − R2) / 18`, `P2 = (2·R2 − R1) / 18`.
pub fn cubic_through_controls(q: &[(f64, f64, f64)]) -> NurbsCurve {
    assert_eq!(q.len(), 4, "genau vier Punkte");
    let v: Vec<DVec3> = q.iter().map(|&(x, y, z)| DVec3::new(x, y, z)).collect();
    let r1 = v[1] * 27.0 - v[0] * 8.0 - v[3];
    let r2 = v[2] * 27.0 - v[0] - v[3] * 8.0;
    let p1 = (r1 * 2.0 - r2) / 18.0;
    let p2 = (r2 * 2.0 - r1) / 18.0;
    let c = NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        vec![v[0], p1, p2, v[3]],
        vec![1.0; 4],
    )
    .unwrap();
    for (i, want) in v.iter().enumerate() {
        let got = c.point(i as f64 / 3.0);
        assert!(
            got.distance(*want) < 1e-9,
            "Kurve läuft nicht durch Punkt {i}"
        );
    }
    c
}

/// Geschlossene kubische Freiformkurve in der XY-Ebene: Anfang und Ende bei (10, 0, 0), dort
/// senkrecht nach +Y laufend; sie umrundet den Ursprung einmal.
pub fn closed_nurbs_loop() -> NurbsCurve {
    let pts = [
        (10.0, 0.0),
        (10.0, 10.0),
        (-10.0, 10.0),
        (-10.0, -10.0),
        (10.0, -10.0),
        (10.0, -5.0),
        (10.0, 0.0),
    ];
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0],
        pts.iter().map(|&(x, y)| DVec3::new(x, y, 0.0)).collect(),
        vec![1.0; 7],
    )
    .unwrap()
}

/// Zahl der Schnittpunkte der projizierten Striche, unabhängig von der Bibliothek gezählt:
/// beide Striche werden an `n + 1` gleichverteilten Parametern abgetastet (eigene Projektion
/// `(P·ex, P·ey)`), dann werden alle Streckenpaare der beiden Polygonzüge geprüft (halboffene
/// Strecken `[0, 1)`, damit ein Schnitt genau in einer Ecke nur einmal zählt). Ein Raster über
/// den Strecken von `b` spart die meisten Paartests.
pub fn brute_force_cross_count(a: &Stroke, b: &Stroke, basis: &Basis, n: usize) -> usize {
    let poly = |s: &Stroke| -> Vec<(f64, f64)> {
        (0..=n)
            .map(|i| {
                let t = s.t0 + (s.t1 - s.t0) * i as f64 / n as f64;
                let p = s.curve.point(t);
                (p.dot(basis.ex), p.dot(basis.ey))
            })
            .collect()
    };
    let (pa, pb) = (poly(a), poly(b));
    let (mut lo, mut hi) = (
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for p in pa.iter().chain(pb.iter()) {
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    }
    const G: usize = 512;
    let cw = ((hi.0 - lo.0) / G as f64).max(1e-12);
    let ch = ((hi.1 - lo.1) / G as f64).max(1e-12);
    let cell = |x: f64, y: f64| {
        let i = (((x - lo.0) / cw).floor().max(0.0) as usize).min(G - 1);
        let j = (((y - lo.1) / ch).floor().max(0.0) as usize).min(G - 1);
        (i, j)
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); G * G];
    let seg_cells = |p: (f64, f64), q: (f64, f64)| {
        let (i0, j0) = cell(p.0.min(q.0), p.1.min(q.1));
        let (i1, j1) = cell(p.0.max(q.0), p.1.max(q.1));
        (i0, j0, i1, j1)
    };
    for k in 0..pb.len() - 1 {
        let (i0, j0, i1, j1) = seg_cells(pb[k], pb[k + 1]);
        for i in i0..=i1 {
            for j in j0..=j1 {
                grid[i * G + j].push(k);
            }
        }
    }
    let cross = |u: (f64, f64), v: (f64, f64)| u.0 * v.1 - u.1 * v.0;
    let sub = |u: (f64, f64), v: (f64, f64)| (u.0 - v.0, u.1 - v.1);
    let mut seen = vec![usize::MAX; pb.len()];
    let mut count = 0;
    for k in 0..pa.len() - 1 {
        let (a0, a1) = (pa[k], pa[k + 1]);
        let (i0, j0, i1, j1) = seg_cells(a0, a1);
        for i in i0..=i1 {
            for j in j0..=j1 {
                for &m in &grid[i * G + j] {
                    if seen[m] == k {
                        continue;
                    }
                    seen[m] = k;
                    let (b0, b1) = (pb[m], pb[m + 1]);
                    let r = sub(a1, a0);
                    let s = sub(b1, b0);
                    let den = cross(r, s);
                    if den == 0.0 {
                        continue;
                    }
                    let w = sub(b0, a0);
                    let u = cross(w, s) / den;
                    let v = cross(w, r) / den;
                    if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

/// Platte `[0, nx·pitch] × [0, ny·pitch] × [0, h]` mit `nx × ny` durchgehenden Rundlöchern
/// (Radius `r`, Achse Z) in den Mitten eines Rasters der Weite `pitch`.
pub fn plate_with_holes(nx: usize, ny: usize, pitch: f64, r: f64, h: f64) -> Solid {
    let mut holes = Vec::with_capacity(nx * ny);
    for i in 0..nx {
        for j in 0..ny {
            let c = DVec2::new((i as f64 + 0.5) * pitch, (j as f64 + 0.5) * pitch);
            holes.push(ProfileLoop::circle(c, r));
        }
    }
    let profile = Profile {
        outer: ProfileLoop::rectangle(
            DVec2::ZERO,
            DVec2::new(nx as f64 * pitch, ny as f64 * pitch),
        ),
        holes,
    }
    .try_oriented()
    .unwrap();
    extrude(&profile, &Frame::WORLD, h, &tol()).unwrap()
}

/// Kleiner eigener Zufallszahlengenerator (SplitMix64) für die Prüfreihe: deterministisch je
/// Startwert, ohne fremdes Paket.
pub struct SeedRng(u64);

impl SeedRng {
    pub fn new(seed: u64) -> SeedRng {
        SeedRng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Ganzzahl in `[0, n)`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// Vielfaches von 0,5 in `[lo, hi]` (beide Vielfache von 0,5).
    pub fn half_steps(&mut self, lo: f64, hi: f64) -> f64 {
        let n = ((hi - lo) * 2.0).round() as u64;
        lo + 0.5 * self.below(n + 1) as f64
    }
}

/// Exakte Drehung (Achsvertauschung) so, dass die Z-Achse des Körpers auf die Weltachse
/// `axis` (0 = X, 1 = Y, 2 = Z) fällt; zyklische Vertauschung, also rechtshändig und ohne
/// Rundung.
fn axis_permuted(s: &Solid, axis: u64) -> Solid {
    use planora_math::{DMat3, DMat4};
    let m = match axis {
        // Spalten = Bilder von X, Y, Z. (x, y, z) → (z, x, y): Z → X.
        0 => DMat4::from_mat3(DMat3::from_cols(DVec3::Y, DVec3::Z, DVec3::X)),
        // (x, y, z) → (y, z, x): Z → Y.
        1 => DMat4::from_mat3(DMat3::from_cols(DVec3::Z, DVec3::X, DVec3::Y)),
        _ => DMat4::IDENTITY,
    };
    mapped(s, &m)
}

/// Grundkörper der Prüfreihe als eigenes Formelmodell (für [`CsgJudge`]): Achse 0/1/2 = X/Y/Z.
#[derive(Clone, Copy, Debug)]
pub enum Prim {
    Box {
        lo: DVec3,
        hi: DVec3,
    },
    Cylinder {
        axis: usize,
        base: DVec3,
        r: f64,
        h: f64,
    },
    Sphere {
        c: DVec3,
        r: f64,
    },
    /// Kegel(stumpf): Grundkreis Radius `r0` bei `base`, Deckel Radius `r1` in Höhe `h`.
    Cone {
        axis: usize,
        base: DVec3,
        r0: f64,
        r1: f64,
        h: f64,
    },
}

impl Prim {
    /// Vorzeichenbehaftete Abstandsschätzung (innen negativ, Lipschitz ≤ 1).
    pub fn f(&self, p: DVec3) -> f64 {
        let axial = |axis: usize, base: DVec3| {
            let l = p - base;
            let z = l[axis];
            let mut q = l;
            q[axis] = 0.0;
            (q.length(), z)
        };
        match *self {
            Prim::Box { lo, hi } => {
                let q = (p - 0.5 * (lo + hi)).abs() - 0.5 * (hi - lo);
                q.max_element()
            }
            Prim::Cylinder { axis, base, r, h } => {
                let (rho, z) = axial(axis, base);
                (rho - r).max(-z).max(z - h)
            }
            Prim::Sphere { c, r } => p.distance(c) - r,
            Prim::Cone {
                axis,
                base,
                r0,
                r1,
                h,
            } => {
                let (rho, z) = axial(axis, base);
                let k = h / h.hypot(r0 - r1);
                ((rho - (r0 + (r1 - r0) * z / h)) * k).max(-z).max(z - h)
            }
        }
    }
}

/// Ein Grundkörper der Prüfreihe mit Maßen in Vielfachen von 0,5 mm, Mitte bei `center`:
/// Quader, Zylinder (Achse X, Y oder Z), Kugel oder Kegel/Kegelstumpf (Achse X, Y oder Z).
/// `scale`: größtes Maß. Liefert den Körper und sein Formelmodell.
fn random_primitive(rng: &mut SeedRng, center: DVec3, scale: f64) -> (Solid, Prim) {
    let small = (scale * 0.5).max(1.0);
    match rng.below(4) {
        0 => {
            let (a, b, c) = (
                rng.half_steps(1.0, scale),
                rng.half_steps(1.0, scale),
                rng.half_steps(1.0, scale),
            );
            // Ecken auf dem 0,5-Raster.
            let lo = DVec3::new(
                ((center.x - a / 2.0) * 2.0).round() / 2.0,
                ((center.y - b / 2.0) * 2.0).round() / 2.0,
                ((center.z - c / 2.0) * 2.0).round() / 2.0,
            );
            let hi = lo + DVec3::new(a, b, c);
            (translated(&box_solid(a, b, c), lo), Prim::Box { lo, hi })
        }
        1 => {
            let r = rng.half_steps(1.0, small);
            let h = rng.half_steps(1.0, scale);
            let axis = rng.below(3) as usize;
            let c = axis_permuted(&cylinder_solid(r, h), axis as u64);
            // Um die halbe Höhe (auf 0,5 abgerundet) zurück, damit die Mitte nahe `center` liegt.
            let mut lift = DVec3::ZERO;
            lift[axis] = -h.floor() / 2.0;
            let base = center + lift;
            (translated(&c, base), Prim::Cylinder { axis, base, r, h })
        }
        2 => {
            let r = rng.half_steps(1.0, small);
            (
                translated(&sphere_solid(r), center),
                Prim::Sphere { c: center, r },
            )
        }
        _ => {
            let r0 = rng.half_steps(1.0, small);
            let r1 = if rng.below(2) == 0 {
                0.0
            } else {
                rng.half_steps(0.5, (r0 - 0.5).max(0.5))
                    .min(r0 - 0.5)
                    .max(0.0)
            };
            let h = rng.half_steps(1.0, scale);
            let axis = rng.below(3) as usize;
            let c = axis_permuted(&cone_solid(r0, r1, h), axis as u64);
            (
                translated(&c, center),
                Prim::Cone {
                    axis,
                    base: center,
                    r0,
                    r1,
                    h,
                },
            )
        }
    }
}

/// Boolesche Verknüpfung, `None` bei Fehler des Kernels, ungültigem oder leerem Ergebnis.
fn try_boolean(a: &Solid, b: &Solid, op: planora_kernel::BoolOp) -> Option<Solid> {
    let out = planora_kernel::boolean::boolean(a, b, op, &tol()).ok()?;
    validate(&out).ok()?;
    (!out.faces.is_empty()).then_some(out)
}

/// Formelmodell eines Zufallskörpers: Grundkörper nacheinander verknüpft (der erste mit
/// `Union` als Anfang).
#[derive(Clone, Debug, Default)]
pub struct Csg {
    pub parts: Vec<(Prim, planora_kernel::BoolOp)>,
}

impl Csg {
    /// Vorzeichenbehaftete Abstandsschätzung des Körpers (innen negativ): Vereinigung `min`,
    /// Differenz `max(f, −g)`.
    pub fn f(&self, p: DVec3) -> f64 {
        let mut f = f64::INFINITY;
        for (prim, op) in &self.parts {
            let g = prim.f(p);
            f = match op {
                planora_kernel::BoolOp::Union => f.min(g),
                planora_kernel::BoolOp::Difference => f.max(-g),
                planora_kernel::BoolOp::Intersection => f.max(g),
            };
        }
        f
    }
}

/// Zufallskörper der Prüfreihe zum Startwert `seed` (deterministisch): ein Grundkörper
/// (Quader, Zylinder, Kugel, Kegel; Maße bis 6…10 mm) um den Ursprung, dann 0–3 Boolesche
/// Verknüpfungen (Vereinigung oder Differenz) mit kleineren Grundkörpern, deren Mitten auf dem
/// 0,5-mm-Raster in der Nähe des Körpers liegen. Alle Maße und Lagen sind Vielfache von 0,5 mm.
/// Lehnt der Kernel eine Verknüpfung ab (etwa Zylinder gegen Zylinder) oder ist ihr Ergebnis
/// ungültig oder leer, entfällt sie (deterministisch).
pub fn random_body(seed: u64) -> Solid {
    random_body_csg(seed).0
}

/// Wie [`random_body`], dazu das Formelmodell des Körpers.
pub fn random_body_csg(seed: u64) -> (Solid, Csg) {
    let mut rng = SeedRng::new(seed);
    let scale = rng_scale(&mut rng, 6.0, 10.0);
    let (mut body, first) = random_primitive(&mut rng, DVec3::ZERO, scale);
    let mut csg = Csg {
        parts: vec![(first, planora_kernel::BoolOp::Union)],
    };
    let ops = rng.below(4);
    for _ in 0..ops {
        let center = DVec3::new(
            rng.half_steps(-4.0, 4.0),
            rng.half_steps(-4.0, 4.0),
            rng.half_steps(-2.0, 6.0),
        );
        let scale = rng_scale(&mut rng, 3.0, 6.0);
        let (tool, prim) = random_primitive(&mut rng, center, scale);
        let op = if rng.below(2) == 0 {
            planora_kernel::BoolOp::Union
        } else {
            planora_kernel::BoolOp::Difference
        };
        if let Some(out) = try_boolean(&body, &tool, op) {
            body = out;
            csg.parts.push((prim, op));
        }
    }
    (body, csg)
}

/// Kippwinkel des exakten Richters (Festlegung F9, wie `visibility::TILT`).
pub const CSG_TILT: f64 = 1e-5;

/// Exaktes Sichturteil am Formelmodell eines Zufallskörpers (unabhängig vom Netz und von der
/// Bibliothek): 8 Prüfstrahlen zum Auge, je um [`CSG_TILT`] gekippt (die Kippung der Festlegung
/// F9; das Netz braucht seine größere Kippung nur wegen der Sehnenabweichung). Längs eines
/// Strahls wird
/// `f = csg.f` in Schritten von `1e-3·E` abgetastet; in jedem Abschnitt, in dem `f` die Schwelle
/// erreichen könnte (`f` ändert sich höchstens um die Schrittweite), sucht der Goldene Schnitt das
/// kleinste `f`. Verdeckt, wenn irgendwo `f < −m` (`m = 0,01 · Kippung · E`); frei, wenn sonst ab
/// `s₁ = 2·m / Kippung` (hinter dem Anfangsband, in dem der Strahl noch an der eigenen Fläche
/// liegt) überall `f > m`. Sichtbar, wenn ein Strahl frei ist; verdeckt, wenn alle verdeckt sind;
/// sonst `None`. Auch Material dünner als jede Netzgenauigkeit (ein Strahl, der eine Kugel knapp
/// innerhalb ihres Umrisses durchläuft) wird so erkannt.
pub struct CsgJudge {
    pub csg: Csg,
    pub extent: f64,
    lo: DVec3,
    hi: DVec3,
}

impl CsgJudge {
    /// `lo`, `hi`: Hüllbox des Körpers.
    pub fn new(csg: Csg, s: &Solid) -> CsgJudge {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for (_, e) in s.edges.iter() {
            for i in 0..=64 {
                let p = e.curve.point(e.t0 + (e.t1 - e.t0) * i as f64 / 64.0);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        for (_, f) in s.faces.iter() {
            if let Surface::Sphere { frame, radius } = f.surface {
                lo = lo.min(frame.origin - DVec3::splat(radius));
                hi = hi.max(frame.origin + DVec3::splat(radius));
            }
        }
        let d = hi - lo;
        let extent = d.x.max(d.y).max(d.z).max(1e-9);
        CsgJudge {
            csg,
            extent,
            lo: lo - DVec3::splat(0.1 * extent),
            hi: hi + DVec3::splat(0.1 * extent),
        }
    }

    /// Kleinstes `f` auf `[a, b]` des Strahls (Goldener Schnitt).
    fn golden(&self, p: DVec3, dir: DVec3, mut a: f64, mut b: f64) -> f64 {
        let g = 0.5 * (5f64.sqrt() - 1.0);
        let f = |s: f64| self.csg.f(p + dir * s);
        let (mut x1, mut x2) = (b - g * (b - a), a + g * (b - a));
        let (mut f1, mut f2) = (f(x1), f(x2));
        for _ in 0..80 {
            if f1 <= f2 {
                b = x2;
                x2 = x1;
                f2 = f1;
                x1 = b - g * (b - a);
                f1 = f(x1);
            } else {
                a = x1;
                x1 = x2;
                f1 = f2;
                x2 = a + g * (b - a);
                f2 = f(x2);
            }
        }
        f1.min(f2)
    }

    /// Urteil eines Strahls mit der Kippung `tilt`: Some(true) frei, Some(false) verdeckt, None
    /// unentschieden. Schwelle `m = 0,01 · tilt · E`.
    fn ray(&self, p: DVec3, dir: DVec3, tilt: f64) -> Option<bool> {
        let m = 1e-2 * tilt * self.extent;
        let s1 = 2.0 * m / tilt;
        let mut s_end = f64::INFINITY;
        for k in 0..3 {
            if dir[k] != 0.0 {
                let a = (self.lo[k] - p[k]) / dir[k];
                let b = (self.hi[k] - p[k]) / dir[k];
                s_end = s_end.min(a.max(b));
            }
        }
        if s_end.is_nan() || s_end <= 0.0 {
            return Some(true);
        }
        let h = 1e-3 * self.extent;
        let n = (s_end / h).ceil().max(1.0) as usize;
        let at = |i: usize| s_end * i as f64 / n as f64;
        let s0 = 1e-9 * self.extent;
        let vals: Vec<f64> = (0..=n)
            .map(|i| self.csg.f(p + dir * at(i).max(s0)))
            .collect();
        let step = s_end / n as f64;
        let mut free = true;
        for i in 0..n {
            let (a, b) = (at(i).max(s0), at(i + 1));
            let (fa, fb) = (vals[i], vals[i + 1]);
            // Kann f in diesem Abschnitt unter m fallen?
            let low = if (fa + fb - step) / 2.0 <= m {
                fa.min(fb).min(self.golden(p, dir, a, b))
            } else {
                fa.min(fb)
            };
            if low < -m {
                return Some(false);
            }
            if b > s1 && low <= m {
                free = false;
            }
        }
        free.then_some(true)
    }

    /// Sichturteil am Punkt `p` für die Ansicht `basis` (siehe Typbeschreibung). Hängt das
    /// Urteil von der Größe der Kippung ab (gleich mit `CSG_TILT` und `CSG_TILT / 100`), sonst
    /// `None`: Nahe einer Stelle, an der die Linie den Umriss eines Verdeckers im Bild berührt,
    /// ist sie nur in einem Streifen der Breite `√(Radius · Kippung · Abstand)` frei – eine
    /// Folge der Kippung, keine Eigenschaft des Körpers.
    pub fn visible(&self, p: DVec3, basis: &Basis) -> Option<bool> {
        let a = self.visible_tilted(p, basis, CSG_TILT)?;
        let b = self.visible_tilted(p, basis, 1e-2 * CSG_TILT)?;
        (a == b).then_some(a)
    }

    fn visible_tilted(&self, p: DVec3, basis: &Basis, tilt: f64) -> Option<bool> {
        let eye = -basis.d;
        let mut all_blocked = true;
        for k in 0..8 {
            let phi = k as f64 * TAU / 8.0;
            let dir = (eye + (basis.ex * phi.cos() + basis.ey * phi.sin()) * tilt).normalize();
            match self.ray(p, dir, tilt) {
                Some(true) => return Some(true),
                Some(false) => {}
                None => all_blocked = false,
            }
        }
        all_blocked.then_some(false)
    }
}

fn rng_scale(rng: &mut SeedRng, lo: f64, hi: f64) -> f64 {
    rng.half_steps(lo, hi)
}

/// Zylinder (Radius `r`, Höhe `h`, Achse Z) mit einer Kerbe an der Seite +X: Quader
/// `x ∈ [r − 2, r + 1]`, `y ∈ [−r − 1, r + 1]`, `z ∈ [z0, z1]` abgezogen.
pub fn notched_cylinder(r: f64, h: f64, z0: f64, z1: f64) -> Solid {
    let cut = box_at((r - 2.0, -r - 1.0, z0), (r + 1.0, r + 1.0, z1));
    boolean_op(
        &cylinder_solid(r, h),
        &cut,
        planora_kernel::BoolOp::Difference,
    )
}

/// Zylinder (Radius `r`, Höhe 300) mit zwei Kerben an der Seite +X (`z ∈ [1, 150]` und
/// `z ∈ [150,5, 299]`): Vom Mantel bleibt bei +X nur ein schmaler Streifen `z ∈ [150, 150,5]`
/// (dazu die Ränder `z ∈ [0, 1]` und `[299, 300]`).
pub fn strip_cylinder(r: f64) -> Solid {
    let a = notched_cylinder(r, 300.0, 1.0, 150.0);
    let cut = box_at((r - 2.0, -r - 1.0, 150.5), (r + 1.0, r + 1.0, 299.0));
    boolean_op(&a, &cut, planora_kernel::BoolOp::Difference)
}

/// Kugel (Radius `big_r`) mit einer kleinen radialen Sackbohrung (Radius `hole_r`, Tiefe 1)
/// in Richtung `dir` (normiert) von außen.
pub fn sphere_with_radial_hole(big_r: f64, hole_r: f64, dir: DVec3) -> Solid {
    let dir = dir.normalize();
    let drill = axis_cylinder(hole_r, 1.0, dir, dir * big_r);
    boolean_op(
        &sphere_solid(big_r),
        &drill,
        planora_kernel::BoolOp::Difference,
    )
}

/// Gerundeter Zylinder aus [`rounded_cylinder_solid`] mit einem schmalen Schlitz: Keil um die
/// Achse Z zwischen den Winkeln `u_world ± half_u` (Weltwinkel), Höhe `z ∈ [z_c − half_z,
/// z_c + half_z]` (innerhalb der Rundung, 7 < z < 10). Auf der Rundung entsteht ein kleines
/// Fenster, begrenzt von Kreisen `z = konst.` und Meridianen (der Kernel schneidet den Torus nur
/// mit waagrechten Ebenen und Ebenen durch die Achse). Der Keil ist der Schnitt zweier um Z
/// gedrehter Quader. Liefert den Körper und den Index der Torusfläche.
pub fn rounded_cylinder_with_window(
    u_world: f64,
    half_u: f64,
    z_c: f64,
    half_z: f64,
) -> (Solid, usize) {
    let (s, _) = rounded_cylinder_solid();
    let (z0, z1, l) = (z_c - half_z, z_c + half_z, 20.0);
    // Quadrant [0°, 90°] um −half_u gedreht und Quadrant [−90°, 0°] um +half_u gedreht.
    let a = rotated(&box_at((0.0, 0.0, z0), (l, l, z1)), DVec3::Z, -half_u);
    let b = rotated(&box_at((0.0, -l, z0), (l, 0.0, z1)), DVec3::Z, half_u);
    let wedge = boolean_op(&a, &b, planora_kernel::BoolOp::Intersection);
    let wedge = rotated(&wedge, DVec3::Z, u_world);
    let out = boolean_op(&s, &wedge, planora_kernel::BoolOp::Difference);
    let face = out
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        .map(|(id, _)| id.index())
        .expect("Torusfläche");
    (out, face)
}

/// Schräger Schlitz (Prüfaufbau der Abschlussprüfung, I1-Rest): Platte
/// `box_at((−l, −l + yo, −s/2), (l, l + yo, s/2))` mit `l = 0,9·r`, `yo = 0,017·r·β²` (β in Grad),
/// um 45° um X gedreht, dann um β um Z, verschoben nach `at`, von `base` abgezogen. Auf einer
/// Geraden längs Z durch `at` ist der Schlitz `s·√2` breit.
pub fn slanted_slot(base: &Solid, at: DVec3, r: f64, s: f64, beta_deg: f64) -> Solid {
    let l = 0.9 * r;
    let yo = 0.017 * r * beta_deg * beta_deg;
    let slab = box_at((-l, -l + yo, -0.5 * s), (l, l + yo, 0.5 * s));
    let slab = rotated(&slab, DVec3::X, std::f64::consts::FRAC_PI_4);
    let slab = rotated(&slab, DVec3::Z, beta_deg.to_radians());
    let slab = translated(&slab, at);
    boolean_op(base, &slab, planora_kernel::BoolOp::Difference)
}

/// Zylinder (Radius `r`, Höhe `h`, Achse Z, Boden im Ursprung) mit einer Sackbohrung auf
/// derselben Achse von oben (Radius `hole_r`, Tiefe `depth`): Drehkörper des Schnitts
/// `(0,0) → (r,0) → (r,h) → (hole_r,h) → (hole_r,h − depth) → (0,h − depth)`. Kreiskanten: Boden
/// (r, z = 0), Deckelrand (r, z = h), Bohrungsrand (hole_r, z = h), Bohrungsgrund
/// (hole_r, z = h − depth) – alle konzentrisch.
pub fn cylinder_with_blind_hole(r: f64, h: f64, hole_r: f64, depth: f64) -> Solid {
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(r, 0.0),
            },
            Segment::Line {
                to: DVec2::new(r, h),
            },
            Segment::Line {
                to: DVec2::new(hole_r, h),
            },
            Segment::Line {
                to: DVec2::new(hole_r, h - depth),
            },
            Segment::Line {
                to: DVec2::new(0.0, h - depth),
            },
        ],
    };
    let s = revolve(&chain, &Frame::WORLD, &tol()).unwrap();
    validate(&s).expect("Zylinder mit Sackbohrung ist gültig");
    s
}

/// Zylinder aus [`cylinder_solid`], dessen Kreiskanten und Zylinderfläche kaputte Zahlen tragen:
/// Kantenradien `edge_r`, Flächenradius `face_r` (etwa 0 oder NaN). Nicht geprüft (absichtlich
/// ungültig) – nur für die Mittellinien, die daran nicht abstürzen dürfen.
pub fn cylinder_with_bad_numbers(edge_r: f64, face_r: f64) -> Solid {
    let mut s = cylinder_solid(5.0, 12.0);
    let ids: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    for id in ids {
        if let Curve::Circle { radius, .. } = &mut s.edges[id].curve {
            *radius = edge_r;
        }
    }
    let ids: Vec<Id<Face>> = s.faces.iter().map(|(id, _)| id).collect();
    for id in ids {
        if let Surface::Cylinder { radius, .. } = &mut s.faces[id].surface {
            *radius = face_r;
        }
    }
    s
}

/// Mittellinien-Sollgeometrie eines Grundkörpers der Prüfreihe aus seinen Erzeugungsdaten:
/// Achsen (Punkt, Weltachse 0/1/2, Höhenbereich `[0, h]` ab `base`) der Zylinder und Kegel und
/// Mittelpunkte der Kugeln. Eigene Rechnung, ohne Code aus `planora-drawing`.
pub enum PrimCenter {
    Axis { base: DVec3, axis: usize, h: f64 },
    Point { c: DVec3 },
}

/// Sollgeometrie aller Grundkörper eines Formelmodells (Quader liefern nichts).
pub fn prim_centers(csg: &Csg) -> Vec<PrimCenter> {
    csg.parts
        .iter()
        .filter_map(|(p, _)| match *p {
            Prim::Box { .. } => None,
            Prim::Cylinder { axis, base, h, .. } | Prim::Cone { axis, base, h, .. } => {
                Some(PrimCenter::Axis { base, axis, h })
            }
            Prim::Sphere { c, .. } => Some(PrimCenter::Point { c }),
        })
        .collect()
}
