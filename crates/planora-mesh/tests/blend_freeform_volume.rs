//! Rundungen und Fasen an Freiformflächen (Teilprojekt 8, Task 15): Jedes `Ok` ist gültig,
//! dicht vernetzbar und hat die Volumenänderung nach einem unabhängigen Orakel.
//!
//! - **Analytische Zwillinge:** Dieselbe Form einmal mit Freiformfläche (Ellipse mit gleichen
//!   Halbachsen extrudiert = Freiform-Zylinder; Viertelkreis bzw. Bogen als NURBS gedreht =
//!   Freiform-Kugel bzw. -Torus), einmal analytisch. Beide werden gegen dieselbe Formel geprüft
//!   (Pappus mit eigener Querschnittsfläche) und gegeneinander.
//! - **Ellipse mit verschiedenen Halbachsen:** Die gerundete Ellipse ist in der Höhe `z` die um
//!   `ρ(z)` aufgeweitete, um `r` geschrumpfte Ellipse (die Kugel rollt innen ab, solange `r` unter
//!   dem kleinsten Krümmungsradius `b²/a` bleibt). Nach der Steiner-Formel konvexer Mengen ist die
//!   abgetragene Fläche in der Höhe `(r − ρ) · (P − 2πr) + π (r² − ρ²)`, integriert
//!   `ΔV = −((P − 2πr) · r² (1 − π/4) + π r³ / 3)`; Fase mit Abstand `d`: `ΔV = −(P d²/2 − π d³/3)`.
//!   Der Umfang `P` wird hier numerisch integriert (unabhängig vom Kernel).
//! - **Spline-Wand gegen ebene Wand (senkrechte Kante):** Prisma über der ebenen Rundung, deren
//!   Kreis hier mit eigener Fußpunktsuche auf dem Spline bestimmt wird.
//!
//! Netzfehler: Freiformflächen werden mit dem Raster der Freiform-Vernetzung geteilt; bei Sehne
//! 1e-3 hebt sich ihr Fehler zwischen Eingang und Ergebnis nicht so gut auf wie bei analytischen
//! Flächen (gemessen bis 8,4e-3 relativ zu ΔV beim Freiform-Zylinder, bei Sehne 1e-4 nur noch
//! 4e-4). Die schnellen Tests prüfen deshalb mit [`FREEFORM_MESH_REL`] und der P2-Schranke, die
//! Reihe `freeform_volumes_at_a_fine_chord` (nur Release) mit Sehne 1e-4 und `MESH_REL` 1e-3.
//! Die Lage der Hautflächen prüft `planora-kernel/tests/blend_freeform.rs` punktweise auf 1e-5.
//!
//! Liegt hier, weil der Kernel nicht vernetzen kann.

mod blend_common;

use blend_common::*;
use planora_kernel::blend::topo::face_name;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use planora_mesh::{
    BLEND_REL, BLEND_SLACK_FACTOR, Discretizer, blend_volume_plausible, check_face_boundaries,
    curved_area, is_watertight, mass_properties, tessellate_with,
};
use std::sync::Arc;

/// Sehnentoleranz der schnellen Tests.
const CHORD: f64 = 1e-3;
/// Relative Schranke der vernetzten Volumenänderung gegen die Formel bei Sehne [`CHORD`] mit
/// Freiformflächen (gemessen höchstens 8,4e-3, siehe Kopf).
const FREEFORM_MESH_REL: f64 = 1e-2;
/// Sehnentoleranz der Release-Reihe.
const FINE_CHORD: f64 = 1e-4;
/// Relative Schranke bei [`FINE_CHORD`] (wie `blend_variable.rs`).
const MESH_REL: f64 = 1e-3;
/// Relative Schranke der Volumenschätzung des Kernels gegen die Formel, wo die Querschnitte von
/// geraden Flächenstücken begrenzt sind (Simpson über exakte Querschnitte; gemessen < 1e-6).
/// Sonst (gekrümmte Fläche im Querschnitt) nähert die Schätzung das Stück zwischen Kante und
/// Berührpunkt durch die Sehne an; dann gilt nur die Volumenprobe des Features.
const ESTIMATE_REL: f64 = 1e-4;
/// Querschnittsfläche der Rundung mit Radius 1 an einer rechtwinkligen Kante.
const A: f64 = 1.0 - PI / 4.0;

/// Relative Netzschranken beim Torusstück (schnell, Release-Reihe): Dort ist ΔV klein gegen die
/// neu geteilte Torusfläche, der Netzfehler entsprechend groß (gemessen, auch beim analytischen
/// Zwilling: Sehne 1e-3 bis 2,5e-2, Sehne 1e-4 3,8e-3, fallend mit der Sehne). Die Lage der
/// Rundungsfläche prüft `planora-kernel/tests/blend_freeform.rs` punktweise auf 1e-5.
const TORUS_MESH_REL: (f64, f64) = (3e-2, 5e-3);

/// Ein geprüfter Fall: Eingang, Ergebnis, Formel für ΔV, Schätzung exakt vergleichbar?
/// `rel`: relative Netzschranken (Sehne [`CHORD`], Sehne [`FINE_CHORD`]).
struct Case {
    what: String,
    before: Solid,
    after: Blended,
    want: f64,
    exact_estimate: bool,
    rel: (f64, f64),
}

/// Prüft einen Fall und gibt das Netzvolumen der Änderung zurück: gültig, Flächenränder, dichtes
/// Netz, ΔV nach Formel innerhalb der P2-Schranke und `rel`, gegebenenfalls Schätzung des Kernels
/// nahe der Formel, Volumenprobe des Features.
fn check(c: &Case, chord: f64, rel: f64) -> f64 {
    let (what, before, b, want) = (&c.what, &c.before, &c.after, c.want);
    validate(&b.solid).unwrap_or_else(|e| panic!("{what}: validate: {e}"));
    let disc = Discretizer::for_solids(&[before, &b.solid], chord);
    let mb = tessellate_with(before, &disc).unwrap();
    let ma = tessellate_with(&b.solid, &disc).unwrap_or_else(|e| panic!("{what}: Netz: {e}"));
    check_face_boundaries(&b.solid, &ma).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(is_watertight(&ma, 1e-6), "{what}: Netz nicht geschlossen");
    let got = mass_properties(&ma).volume - mass_properties(&mb).volume;
    let slack = BLEND_SLACK_FACTOR
        * disc.chord_tolerance()
        * (curved_area(before, &mb) + curved_area(&b.solid, &ma));
    assert!(
        (got - want).abs() <= slack.min(rel * want.abs()),
        "{what}: ΔV {got} statt {want} (Abweichung {}, Schranke {slack})",
        (got - want).abs()
    );
    if c.exact_estimate {
        assert!(
            (b.volume_change - want).abs() <= ESTIMATE_REL * want.abs(),
            "{what}: Schätzung {} statt {want}",
            b.volume_change
        );
    }
    assert!(
        blend_volume_plausible(
            mass_properties(&mb).volume,
            mass_properties(&ma).volume,
            b.volume_change,
            b.volume_scale,
            BLEND_REL,
            slack,
        ),
        "{what}: Volumenprobe"
    );
    got
}

fn case(what: &str, before: &Solid, after: Blended, want: f64, exact_estimate: bool) -> Case {
    Case {
        what: what.into(),
        before: before.clone(),
        after,
        want,
        exact_estimate,
        rel: (FREEFORM_MESH_REL, MESH_REL),
    }
}

/// Hat der Körper eine Freiformfläche mit der Rolle `role`?
fn freeform_role(s: &Solid, role: &str) -> bool {
    s.faces
        .iter()
        .any(|(id, f)| matches!(f.surface, Surface::Nurbs(_)) && face_name(s, id) == role)
}

/// Extrudierte Ellipse mit Halbachsen `a` (x) und `b` (y), Höhe `h`.
fn ellipse_prism(a: f64, b: f64, h: f64) -> Solid {
    let c = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, b, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    k().extrude(
        &Profile {
            outer: ProfileLoop::nurbs_loop(c).unwrap(),
            holes: vec![],
        },
        &Frame::WORLD,
        h,
    )
    .unwrap()
}

/// Simpson über `[a, b]` mit `n` (gerade) Abschnitten.
fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        sum += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

/// Umfang der Ellipse: Simpson über `sqrt(a² sin² t + b² cos² t)` (glatt und periodisch).
fn ellipse_perimeter(a: f64, b: f64) -> f64 {
    simpson(0.0, TAU, 20_000, |t| {
        (a * a * t.sin().powi(2) + b * b * t.cos().powi(2)).sqrt()
    })
}

/// Abgetragenes Volumen einer Rundung `r` am Rand eines konvexen Prismas mit Umfang `p`
/// (Steiner-Formel, siehe Kopf).
fn rim_fillet_volume(p: f64, r: f64) -> f64 {
    (p - TAU * r) * r * r * A + PI * r * r * r / 3.0
}

/// Abgetragenes Volumen einer Fase `d` am Rand eines konvexen Prismas mit Umfang `p`.
fn rim_chamfer_volume(p: f64, d: f64) -> f64 {
    p * d * d / 2.0 - PI * d * d * d / 3.0
}

/// Schwerpunktabstand des Zwickels (Radius 1) von jeder der beiden Kantenflächen.
fn spandrel_offset() -> f64 {
    (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI)
}

fn fillet(r: f64) -> BlendKind {
    BlendKind::Fillet { radius: r }
}

/// Fläche und Schwerpunkt eines Vielecks (Schnürsenkelformel).
fn shoelace(pts: &[DVec2]) -> (f64, DVec2) {
    let n = pts.len();
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let cr = p.perp_dot(q);
        area += cr;
        cx += (p.x + q.x) * cr;
        cy += (p.y + q.y) * cr;
    }
    area *= 0.5;
    (area.abs(), DVec2::new(cx / (6.0 * area), cy / (6.0 * area)))
}

/// Zylinderrand (R = 5, h = 8): Freiform-Zylinder und analytischer Zwilling, Rundung r = 1 und
/// Fase d = 1 nach Pappus (Fase: Dreieck d²/2, Schwerpunkt d/3 von der Wand).
fn cylinder_cases() -> Vec<Case> {
    let (big, h, r, d) = (5.0, 8.0, 1.0, 1.0);
    let s = ellipse_prism(big, big, h);
    let twin = k().make_cylinder(big, h).unwrap();
    let fillet_want = -A * r * r * TAU * (big - spandrel_offset() * r);
    let chamfer_want = -(d * d / 2.0) * TAU * (big - d / 3.0);
    let mut out = Vec::new();
    for (kind, want, what) in [
        (fillet(r), fillet_want, "Rundung"),
        (
            BlendKind::ChamferEqual { distance: d },
            chamfer_want,
            "Fase",
        ),
    ] {
        for (body, who) in [(&s, "Freiform"), (&twin, "Zwilling")] {
            let e = edge_between(body, "Wand 1", "Deckel");
            let b = blend_edges(body, &[e], kind.clone(), false).unwrap();
            if who == "Freiform" {
                assert!(freeform_role(&b.solid, &format!("{what} 1")), "{what}");
            }
            // Die analytische Schätzung nähert den Bogen durch ein Vieleck (Pappus); exakt
            // vergleichbar ist nur die der Bahn (Simpson über exakte Querschnitte).
            out.push(case(
                &format!("{who}-{what}"),
                body,
                b,
                want,
                who == "Freiform",
            ));
        }
    }
    out
}

/// Kubisches Hermite-Stück auf `[0, 1]` mit Werten `r0`, `r1`, Steigungen `m0`, `m1` (je
/// Längeneinheit der Position) und Stücklänge `h`.
fn hermite(t: f64, h: f64, (r0, m0): (f64, f64), (r1, m1): (f64, f64)) -> f64 {
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * r0
        + (t3 - 2.0 * t2 + t) * h * m0
        + (-2.0 * t3 + 3.0 * t2) * r1
        + (t3 - t2) * h * m1
}

/// Variabler Radius (0, 0,8), (0,5, 1,2), (1, 0,8) am Zylinderrand, Freiform und Zwilling.
/// Gesetz von Hand (Fritsch–Butland: innen Vorzeichenwechsel → 0, Enden Streckensteigung ±0,8);
/// Formel nach Pappus `∫ A r(φ)² (R − x̄ r(φ)) dφ` (Querschnitte in Meridianebenen).
fn variable_cases() -> Vec<Case> {
    let (big, h) = (5.0, 8.0);
    let law = |f: f64| {
        if f <= 0.5 {
            hermite(f / 0.5, 0.5, (0.8, 0.8), (1.2, 0.0))
        } else {
            hermite((f - 0.5) / 0.5, 0.5, (1.2, 0.0), (0.8, -0.8))
        }
    };
    let want = -simpson(0.0, 1.0, 4000, |f| {
        let r = law(f);
        A * r * r * (big - spandrel_offset() * r) * TAU
    });
    let kind = BlendKind::VariableFillet {
        points: vec![(0.0, 0.8), (0.5, 1.2), (1.0, 0.8)],
    };
    let s = ellipse_prism(big, big, h);
    let twin = k().make_cylinder(big, h).unwrap();
    [(&s, "Freiform variabel"), (&twin, "Zwilling variabel")]
        .into_iter()
        .map(|(body, what)| {
            let e = edge_between(body, "Wand 1", "Deckel");
            let b = blend_edges(body, &[e], kind.clone(), false).unwrap();
            case(what, body, b, want, true)
        })
        .collect()
}

/// Ellipse 5 × 3, Höhe 4 (kleinster Krümmungsradius b²/a = 1,8): Rundungen 0,5 und 1,5, Fasen
/// 0,5 und 1,2 nach der Steiner-Formel.
fn ellipse_cases() -> Vec<Case> {
    let (a, b, h) = (5.0, 3.0, 4.0);
    let s = ellipse_prism(a, b, h);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let p = ellipse_perimeter(a, b);
    // Gegenprobe des Umfangs mit der Näherung von Ramanujan (hier auf ~1e-6 genau).
    let hh = ((a - b) / (a + b)).powi(2);
    let ramanujan = PI * (a + b) * (1.0 + 3.0 * hh / (10.0 + (4.0 - 3.0 * hh).sqrt()));
    assert!((p - ramanujan).abs() < 1e-5 * p, "{p} / {ramanujan}");
    let mut out = Vec::new();
    for r in [0.5, 1.5] {
        let bl = blend_edges(&s, &[e], fillet(r), false).unwrap();
        out.push(case(
            &format!("Ellipse Rundung {r}"),
            &s,
            bl,
            -rim_fillet_volume(p, r),
            true,
        ));
    }
    for d in [0.5, 1.2] {
        let bl = blend_edges(&s, &[e], BlendKind::ChamferEqual { distance: d }, false).unwrap();
        out.push(case(
            &format!("Ellipse Fase {d}"),
            &s,
            bl,
            -rim_chamfer_volume(p, d),
            true,
        ));
    }
    out
}

/// Drehkörper aus `start` und den Segmenten (Achse = y-Achse der Skizze, Profil in x ≥ 0).
fn revolved(start: DVec2, segments: Vec<Segment>) -> Solid {
    k().revolve(&ProfileLoop { start, segments }, &Frame::WORLD)
        .unwrap()
}

/// NURBS-Bogen in der Skizzenebene: Mittelpunkt `c`, Halbachsen `x`, `y` (gleich lang =
/// Kreisbogen), Winkel `0 … sweep`.
fn nurbs_arc(c: DVec2, x: DVec2, y: DVec2, sweep: f64) -> Segment {
    Segment::Nurbs {
        curve: Arc::new(
            NurbsCurve::elliptic_arc(c.extend(0.0), x.extend(0.0), y.extend(0.0), 0.0, sweep)
                .unwrap(),
        ),
    }
}

/// Halbkugel R = 5 (Boden „Fläche 1“, Kugel „Fläche 2“) als Freiform und analytisch, Rundung
/// r = 1; Orakel Pappus mit der fein abgetasteten Querschnittsfläche (`plane_circle_spandrel`).
fn sphere_cases() -> Vec<Case> {
    let (big, r) = (5.0, 1.0);
    let sphere = revolved(
        DVec2::ZERO,
        vec![
            Segment::Line {
                to: DVec2::new(big, 0.0),
            },
            nurbs_arc(
                DVec2::ZERO,
                DVec2::new(big, 0.0),
                DVec2::new(0.0, big),
                PI / 2.0,
            ),
        ],
    );
    let twin = hemisphere(big);
    let (area, c) = plane_circle_spandrel(big, r);
    let want = -area * TAU * c.x;
    [(&sphere, "Freiform-Kugel"), (&twin, "Kugel")]
        .into_iter()
        .map(|(body, what)| {
            let e = edge_between(body, "Fläche 1", "Fläche 2");
            let b =
                blend_edges(body, &[e], fillet(r), false).unwrap_or_else(|x| panic!("{what}: {x}"));
            case(what, body, b, want, false)
        })
        .collect()
}

/// Torusstück: Boden (Fläche 1, Kreisscheibe ρ ≤ 5), Viertelkreis um (7, 0) von (5, 0) nach
/// (7, 2) (Fläche 2; Material außerhalb des Kreises, die Kugel liegt auf seiner konvexen Seite),
/// Deckel y = 2. Kante bei ρ = 5 rechtwinklig, Rundung r = 0,5. Orakel: Querschnitt in der
/// Meridianebene (Kugelmittelpunkt (7 − √((2 + r)² − r²), r)) fein abgetastet, Pappus.
fn torus_cases() -> Vec<Case> {
    let r: f64 = 0.5;
    let base = |arc: Segment| {
        revolved(
            DVec2::ZERO,
            vec![
                Segment::Line {
                    to: DVec2::new(5.0, 0.0),
                },
                arc,
                Segment::Line {
                    to: DVec2::new(0.0, 2.0),
                },
            ],
        )
    };
    let ring = DVec2::new(7.0, 0.0);
    let torus = base(nurbs_arc(
        ring,
        DVec2::new(-2.0, 0.0),
        DVec2::new(0.0, 2.0),
        PI / 2.0,
    ));
    let twin = base(Segment::Arc {
        center: ring,
        sweep: -PI / 2.0,
    });
    let center = DVec2::new(7.0 - ((2.0 + r) * (2.0 + r) - r * r).sqrt(), r);
    let q = ring + (center - ring).normalize() * 2.0;
    const N: usize = 4000;
    let mut pts = vec![DVec2::new(5.0, 0.0), DVec2::new(center.x, 0.0)];
    let (a0, a1) = (-PI / 2.0, (q - center).y.atan2((q - center).x));
    for i in 1..N {
        let ang = a0 + (a1 - a0) * i as f64 / N as f64;
        pts.push(center + DVec2::new(ang.cos(), ang.sin()) * r);
    }
    // Auf dem Ringkreis von q zurück zu (5, 0) (Winkel π).
    let (b0, b1) = ((q - ring).y.atan2((q - ring).x), PI);
    for i in 0..N {
        let ang = b0 + (b1 - b0) * i as f64 / N as f64;
        pts.push(ring + DVec2::new(ang.cos(), ang.sin()) * 2.0);
    }
    let (area, c) = shoelace(&pts);
    let want = -area * TAU * c.x;
    [(&torus, "Freiform-Torus"), (&twin, "Torus")]
        .into_iter()
        .map(|(body, what)| {
            let e = edge_between(body, "Fläche 1", "Fläche 2");
            let b =
                blend_edges(body, &[e], fillet(r), false).unwrap_or_else(|x| panic!("{what}: {x}"));
            Case {
                rel: TORUS_MESH_REL,
                ..case(what, body, b, want, false)
            }
        })
        .collect()
}

/// Spline durch (10, 0), (12, 5), (10, 10).
fn spline_curve() -> NurbsCurve {
    NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 1.0, 2.0],
    )
    .unwrap()
}

/// Profil mit einer Spline-Wand (Wand 2) zwischen zwei ebenen Wänden, Höhe `h`.
fn spline_prism(h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(spline_curve()),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

/// Nächster Kurvenparameter zu `p` auf dem Spline: dichte Abtastung, dann Newton auf
/// `(C(t) − p) · C′(t) = 0` (nur Mathematik-Bibliothek, nicht die Rundung des Kernels).
fn spline_foot(c: &NurbsCurve, p: DVec2) -> (f64, DVec2) {
    let (t0, t1) = c.domain();
    let at = |t: f64| c.point(t).truncate();
    let mut t = (0..=2000)
        .map(|i| t0 + (t1 - t0) * i as f64 / 2000.0)
        .min_by(|x, y| at(*x).distance(p).total_cmp(&at(*y).distance(p)))
        .unwrap();
    for _ in 0..50 {
        let d = c.derivs(t, 2);
        let (q, d1, d2) = (d[0].truncate(), d[1].truncate(), d[2].truncate());
        let f = (q - p).dot(d1);
        let df = d1.dot(d1) + (q - p).dot(d2);
        t = (t - f / df).clamp(t0, t1);
    }
    (t, at(t))
}

/// Senkrechte Kante zwischen Wand 1 (y = 0) und der Spline-Wand bei (10, 0), Rundung r = 1: Die
/// Querschnitte sind waagerecht, das Abgetragene ist ein Prisma über der ebenen Zwickelfläche.
/// Der Kreis (Mittelpunkt (cx, r)) berührt y = 0 und den Spline; cx per Bisektion auf
/// `Abstand zum Spline − r`.
fn spline_cases() -> Vec<Case> {
    let (h, r) = (5.0, 1.0);
    let s = spline_prism(h);
    let c = spline_curve();
    let gap = |cx: f64| {
        spline_foot(&c, DVec2::new(cx, r))
            .1
            .distance(DVec2::new(cx, r))
            - r
    };
    let (mut lo, mut hi): (f64, f64) = (5.0, 10.0);
    assert!(gap(lo) > 0.0 && gap(hi) < 0.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if gap(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let center = DVec2::new(0.5 * (lo + hi), r);
    let (tq, q) = spline_foot(&c, center);
    // Zwickel: Ecke (10, 0) → Berührpunkt (cx, 0) → Bogen zum Fußpunkt q → Spline zurück.
    const N: usize = 4000;
    let mut pts = vec![DVec2::new(10.0, 0.0), DVec2::new(center.x, 0.0)];
    let (a0, a1) = (-PI / 2.0, (q - center).y.atan2((q - center).x));
    for i in 1..N {
        let ang = a0 + (a1 - a0) * i as f64 / N as f64;
        pts.push(center + DVec2::new(ang.cos(), ang.sin()) * r);
    }
    let (t0, _) = c.domain();
    for i in 0..N {
        let t = tq + (t0 - tq) * i as f64 / N as f64;
        pts.push(c.point(t).truncate());
    }
    let (area, _) = shoelace(&pts);
    let e = edge_between(&s, "Wand 1", "Wand 2");
    let b = blend_edges(&s, &[e], fillet(r), false).unwrap();
    assert!(freeform_role(&b.solid, "Rundung 1"));
    vec![case("Spline-Wand", &s, b, -area * h, false)]
}

/// Prüft die Fälle bei [`CHORD`] und die Zwillingspaare (Freiform, analytisch) gegeneinander.
fn check_fast(cases: &[Case], twins: bool) {
    let got: Vec<f64> = cases.iter().map(|c| check(c, CHORD, c.rel.0)).collect();
    if twins {
        for (pair, g) in cases.chunks(2).zip(got.chunks(2)) {
            assert!(
                (g[0] - g[1]).abs() <= pair[0].rel.0 * pair[0].want.abs(),
                "{} / {}: {g:?}",
                pair[0].what,
                pair[1].what
            );
        }
    }
}

#[test]
fn a_freeform_cylinder_rim_matches_its_analytic_twin_and_pappus() {
    check_fast(&cylinder_cases(), true);
}

#[test]
fn a_variable_radius_on_a_freeform_cylinder_matches_its_analytic_twin() {
    check_fast(&variable_cases(), true);
}

#[test]
fn ellipse_rims_match_the_offset_formula() {
    check_fast(&ellipse_cases(), false);
}

#[test]
fn freeform_sphere_and_torus_bases_match_their_analytic_twins() {
    check_fast(&sphere_cases(), true);
    check_fast(&torus_cases(), true);
}

#[test]
fn a_spline_wall_edge_matches_the_planar_fillet_area() {
    check_fast(&spline_cases(), false);
}

#[test]
#[ignore = "feine Sehne 1e-4, nur mit --release --ignored (etwa 270 s)"]
fn freeform_volumes_at_a_fine_chord() {
    // Nur die Freiform-Fälle (die analytischen Zwillinge prüfen die Tests der Stufen 1 bis 3).
    let all: Vec<Case> = [
        cylinder_cases(),
        variable_cases(),
        ellipse_cases(),
        sphere_cases(),
        torus_cases(),
        spline_cases(),
    ]
    .into_iter()
    .flatten()
    .filter(|c| {
        c.what.contains("Freiform") || c.what.contains("Ellipse") || c.what.contains("Spline")
    })
    .collect();
    for c in &all {
        let got = check(c, FINE_CHORD, c.rel.1);
        eprintln!(
            "{}: ΔV {got} Formel {} relativ {:.2e} Schätzung {}",
            c.what,
            c.want,
            ((got - c.want) / c.want).abs(),
            c.after.volume_change
        );
    }
}

/// Ellipse 5 × 3, Höhe 4, Rundung r = 0,8 am Deckel: Die Berührkurve auf dem Deckel liegt um
/// 0,8 nach innen versetzt (am Scheitel x = 4,2), die auf der Wand bei z = 3,2.
#[test]
fn holes_and_notches_near_a_freeform_fillet_are_refused_or_exact() {
    let (a, b, h, r) = (5.0, 3.0, 4.0, 0.8);
    let s = ellipse_prism(a, b, h);
    let p = ellipse_perimeter(a, b);
    let pin = |x: f64, depth: f64| {
        k().transform(
            &k().make_cylinder(0.2, depth + 1.0).unwrap(),
            &DMat4::from_translation(DVec3::new(x, 0.0, h - depth)),
        )
        .unwrap()
    };
    // Sackloch im Deckel innerhalb des abgetragenen Streifens (x = 4,6): benannt.
    let holed = k().boolean(&s, &pin(4.6, 1.0), BoolOp::Difference).unwrap();
    let e = edge_between(&holed, "Wand 1", "Deckel");
    match blend_edges(&holed, &[e], fillet(r), false) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("Bereich der Rundung"), "{m}"),
        other => panic!("Loch im Streifen: {:?}", other.map(|_| ())),
    }
    // Dasselbe Loch weiter innen (x = 3,6, Rand bei 3,8 < 4,2): Ok, ΔV nach der Formel.
    let holed = k().boolean(&s, &pin(3.6, 1.0), BoolOp::Difference).unwrap();
    let e = edge_between(&holed, "Wand 1", "Deckel");
    let bl = blend_edges(&holed, &[e], fillet(r), false).unwrap();
    check(
        &case("Loch innen", &holed, bl, -rim_fillet_volume(p, r), true),
        CHORD,
        FREEFORM_MESH_REL,
    );
    // Kerbe in der Wand am Scheitel, die die Berührkurve der Wand (z = 3,2) schneidet: benannt
    // (die Berührkurve verlässt Wand 1 im Loch der Kerbe: zu groß für Wand 1; oder die Kerbe
    // reicht in den Bereich der Rundung).
    let notch = k()
        .transform(
            &k().make_box(DVec3::new(1.0, 0.6, 1.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(4.5, -0.3, 2.7)),
        )
        .unwrap();
    let notched = k().boolean(&s, &notch, BoolOp::Difference).unwrap();
    let e = edge_between(&notched, "Wand 1", "Deckel");
    let r2 = blend_edges(&notched, &[e], fillet(r), false);
    assert!(
        matches!(&r2, Err(BlendError::Unsupported(m)) if m.contains("Bereich der Rundung"))
            || matches!(&r2, Err(BlendError::TooLarge { face }) if face == "Wand 1"),
        "Kerbe: {:?}",
        r2.map(|_| ())
    );
}

/// Übrige Flächenpaare (Task 15): Ellipsenkante zwischen Zylinderwand und schräger Ebene
/// (Zylinder R = 3 um 30° um y gekippt bei z = 6 abgeschnitten), Rundung r = 0,5 über die Bahn.
/// Lage der Hautfläche punktweise in `planora-kernel/tests/blend_freeform.rs`; hier gültig, dicht
/// vernetzbar und Volumenprobe des Features (keine geschlossene Formel).
#[test]
fn an_oblique_ellipse_edge_meshes_and_passes_the_volume_probe() {
    let cyl = k().make_cylinder(3.0, 10.0).unwrap();
    let cutter = k()
        .transform(
            &k().make_box(DVec3::splat(20.0)).unwrap(),
            &(DMat4::from_translation(DVec3::new(0.0, 0.0, 6.0))
                * DMat4::from_rotation_y(PI / 6.0)
                * DMat4::from_translation(DVec3::new(-10.0, -10.0, 0.0))),
        )
        .unwrap();
    let cut = k().boolean(&cyl, &cutter, BoolOp::Difference).unwrap();
    let e = cut
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let ed = &cut.edges[id];
            !matches!(ed.curve, Curve::Circle { .. } | Curve::Line { .. })
                && ed.curve.point(0.5 * (ed.t0 + ed.t1)).z > 3.0
        })
        .unwrap();
    for kind in [fillet(0.5), BlendKind::ChamferEqual { distance: 0.5 }] {
        let b = blend_edges(&cut, &[e], kind.clone(), false)
            .unwrap_or_else(|x| panic!("{kind:?}: {x}"));
        assert!(b.volume_change < 0.0);
        let dv = delta_volume(&cut, &b.solid, CHORD);
        assert!(dv < 0.0, "{kind:?}: {dv}");
        assert!(plausible(&cut, &b, CHORD), "{kind:?}: Volumenprobe");
    }
}
