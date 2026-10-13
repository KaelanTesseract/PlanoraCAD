//! Variabler Radius (Teilprojekt 8, Task 14): Jedes `Ok` ist gültig, dicht vernetzbar und hat
//! die Volumenänderung nach einer unabhängigen Formel (Querschnitte senkrecht zur Kante:
//! gerade Kante `∫ (1 − π/4) r(s)² ds`, Kreiskante nach Pappus `∫ A(r) · ρ_S(r) dφ`). Die
//! Radiengesetze der Formeln sind von Hand aufgeschrieben (Hermite-Stücke mit den Steigungen nach
//! Fritsch–Butland), nicht aus `RadiusLaw` übernommen. Aussparungen, Löcher und Taschen im
//! Bereich der Hautfläche werden benannt abgelehnt (dieselben Prüfungen wie bei analytischen
//! Rundungen). Liegt hier, weil der Kernel nicht vernetzen kann.

mod blend_common;

use blend_common::*;
use planora_kernel::blend::march::VARIABLE_CORNER;
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use planora_mesh::{BLEND_SLACK_FACTOR, Discretizer, curved_area, tessellate_with};

/// Sehnentoleranz der Volumenprobe.
const CHORD: f64 = 1e-3;
/// Relative Schranke der vernetzten Volumenänderung gegen die Formel (enger als die P2-Schranke;
/// bei Sehne 1e-3 gemessen höchstens 4,4e-4, das Netz liegt innen und trägt etwas mehr ab).
const MESH_REL: f64 = 1e-3;
/// Relative Schranke der Volumenschätzung des Kernels (Simpson über exakte Querschnitte;
/// gemessen höchstens 7,4e-6).
const ESTIMATE_REL: f64 = 1e-4;
/// Querschnittsfläche der Rundung mit Radius 1 an einer rechtwinkligen Kante.
const A: f64 = 1.0 - PI / 4.0;

/// Schwerpunktabstand des Zwickels (Radius 1) von jeder der beiden Kantenflächen.
fn spandrel_offset() -> f64 {
    (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI)
}

/// Kubisches Hermite-Stück auf `[0, 1]` mit Werten `r0`, `r1` und Steigungen `m0`, `m1` (je
/// Längeneinheit der Position), Stücklänge `h`.
fn hermite(t: f64, h: f64, (r0, m0): (f64, f64), (r1, m1): (f64, f64)) -> f64 {
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * r0
        + (t3 - 2.0 * t2 + t) * h * m0
        + (-2.0 * t3 + 3.0 * t2) * r1
        + (t3 - t2) * h * m1
}

/// Gesetz `(0, 1), (0,5, 2), (1, 1,5)` von Hand: Steigungen 2 und −1 wechseln das Vorzeichen,
/// also innen 0; an den Enden die Streckensteigungen 2 bzw. −1.
fn hand_law(f: f64) -> f64 {
    if f <= 0.5 {
        hermite(f / 0.5, 0.5, (1.0, 2.0), (2.0, 0.0))
    } else {
        hermite((f - 0.5) / 0.5, 0.5, (2.0, 0.0), (1.5, -1.0))
    }
}

/// Gesetz `(0, 1), (0,5, 1,5), (1, 1)` von Hand (Steigungen 1, 0, −1).
fn seam_law(f: f64) -> f64 {
    if f <= 0.5 {
        hermite(f / 0.5, 0.5, (1.0, 1.0), (1.5, 0.0))
    } else {
        hermite((f - 0.5) / 0.5, 0.5, (1.5, 0.0), (1.0, -1.0))
    }
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

/// Schranke der Volumenprobe (mm³) nach P2: Sehnentoleranz mal `BLEND_SLACK_FACTOR` mal
/// gekrümmter Fläche des Ergebnisses.
fn slack(after: &Solid) -> f64 {
    let disc = Discretizer::new(after, CHORD);
    let mesh = tessellate_with(after, &disc).unwrap();
    BLEND_SLACK_FACTOR * CHORD * curved_area(after, &mesh)
}

fn variable(points: &[(f64, f64)]) -> BlendKind {
    BlendKind::VariableFillet {
        points: points.to_vec(),
    }
}

/// Prüft ein `Ok`: gültig, dicht, Volumenänderung nach Formel innerhalb der Sehnenschranke, die
/// Schätzung des Kernels nahe der Formel und die Volumenprobe des Features besteht.
fn check_ok(before: &Solid, b: &Blended, want: f64, what: &str) {
    let got = delta_volume(before, &b.solid, CHORD);
    let bound = slack(&b.solid);
    assert!(
        (got - want).abs() <= bound.min(MESH_REL * want.abs()),
        "{what}: ΔV {got} statt {want} (Abweichung {}, Schranke {bound})",
        (got - want).abs()
    );
    assert!(
        (b.volume_change - want).abs() <= ESTIMATE_REL * want.abs(),
        "{what}: Schätzung {} statt {want}",
        b.volume_change
    );
    assert!(plausible(before, b, CHORD), "{what}: Volumenprobe");
    assert!(
        b.solid
            .faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Nurbs(_))),
        "{what}: keine Hautfläche"
    );
}

#[test]
fn linear_and_cubic_laws_on_a_box_edge_match_the_integral() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    // Linear: (1 − π/4) · ∫₀¹⁰ (1 + s/10)² ds = (1 − π/4) · 10 · 7/3.
    let b = blend_edges(&s, &[e], variable(&[(0.0, 1.0), (1.0, 2.0)]), false).unwrap();
    check_ok(&s, &b, -A * 10.0 * 7.0 / 3.0, "linear");
    // Kubisch: Gauß-Legendre mit 4 Punkten je Stück ist für r² (Grad 6) exakt.
    let b = blend_edges(
        &s,
        &[e],
        variable(&[(0.0, 1.0), (0.5, 2.0), (1.0, 1.5)]),
        false,
    )
    .unwrap();
    let gauss = [
        (-0.861_136_311_594_052_6, 0.347_854_845_137_453_86),
        (-0.339_981_043_584_856_26, 0.652_145_154_862_546_1),
        (0.339_981_043_584_856_26, 0.652_145_154_862_546_1),
        (0.861_136_311_594_052_6, 0.347_854_845_137_453_86),
    ];
    let mut integral = 0.0;
    for (a, b) in [(0.0, 5.0), (5.0, 10.0)] {
        let (m, h) = (0.5 * (a + b), 0.5 * (b - a));
        for (x, w) in gauss {
            integral += w * h * hand_law((m + h * x) / 10.0).powi(2);
        }
    }
    // Gegenprobe der Gauß-Summe mit Simpson.
    let simp = simpson(0.0, 10.0, 4000, |s| hand_law(s / 10.0).powi(2));
    assert!((integral - simp).abs() < 1e-9, "{integral} / {simp}");
    check_ok(&s, &b, -A * integral, "kubisch");
}

#[test]
fn the_skin_lies_on_the_rolling_ball_sections() {
    // Unabhängig von der Bahn: An einer Würfelkante (Ebenen senkrecht) liegt der Kugelmittelpunkt
    // bei der Stelle s im Abstand r(s) von beiden Ebenen; jeder Punkt der Rundungsfläche hat von
    // ihm den Abstand r(s), und die Berührkurven liegen in den Ebenen im Abstand r(s) von der Kante.
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let edge = &s.edges[e];
    let (p0, p1) = (s.vertices[edge.start].point, s.vertices[edge.end].point);
    let dir = (p1 - p0).normalize();
    let b = blend_edges(
        &s,
        &[e],
        variable(&[(0.0, 1.0), (0.5, 2.0), (1.0, 1.5)]),
        false,
    )
    .unwrap();
    // Würfel [0, 10]³: Deckel z = 10, Wand 1 y = 0; innen liegt +y, −z.
    let inward = DVec3::new(0.0, 1.0, -1.0);
    let center = |p: DVec3| {
        let along = (p - p0).dot(dir);
        let r = hand_law(along / 10.0);
        (p0 + dir * along + inward * r, r)
    };
    let (_, face) = b
        .solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .unwrap();
    let patch = face.patch.unwrap();
    let mut worst: f64 = 0.0;
    for i in 0..=40 {
        for j in 0..=10 {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / 40.0;
            let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / 10.0;
            let p = face.surface.point(u, v);
            let (c, r) = center(p);
            worst = worst.max((p.distance(c) - r).abs());
            // Im Querschnitt zwischen den Berührpunkten (Viertelkreis zur Kante hin).
            assert!(p.y <= c.y + 1e-5 && p.z >= c.z - 1e-5, "{p}");
        }
    }
    assert!(worst < 1e-5, "größte Abweichung {worst}");
    // Berührkurven: neue NURBS-Kanten auf Deckel bzw. Wand 1.
    let mut contacts = 0;
    for (_, ed) in b.solid.edges.iter() {
        if !matches!(ed.curve, Curve::Nurbs(_)) {
            continue;
        }
        contacts += 1;
        for k in 0..=50 {
            let p = ed.curve.point(ed.t0 + (ed.t1 - ed.t0) * k as f64 / 50.0);
            let r = hand_law((p - p0).dot(dir) / 10.0);
            let on_top = (p.z - 10.0).abs() < 1e-5 && (p.y - r).abs() < 1e-5;
            let on_wall = p.y.abs() < 1e-5 && (10.0 - p.z - r).abs() < 1e-5;
            assert!(on_top || on_wall, "{p} (r {r})");
        }
    }
    assert_eq!(contacts, 2);
}

#[test]
fn a_closed_circle_edge_follows_pappus() {
    // Zylinder R = 5, Deckelkante, Gesetz (0, 1), (0,5, 1,5), (1, 1): Querschnitte in den
    // Meridianebenen, also dV = A(r) · (R − x̄ r) dφ (x̄ r = Schwerpunktabstand von der Wand).
    let big = 5.0;
    let c = k().make_cylinder(big, 8.0).unwrap();
    let e = edge_between(&c, "Wand 1", "Deckel");
    let b = blend_edges(
        &c,
        &[e],
        variable(&[(0.0, 1.0), (0.5, 1.5), (1.0, 1.0)]),
        false,
    )
    .unwrap();
    let want = -simpson(0.0, TAU, 4000, |phi| {
        let r = seam_law(phi / TAU);
        A * r * r * (big - spandrel_offset() * r)
    });
    check_ok(&c, &b, want, "Zylinderdeckel");
}

#[test]
fn a_concave_edge_adds_the_integral() {
    // Innenkante eines L-Profils (Höhe 5), Gesetz von Hand: Material kommt hinzu.
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let b = blend_edges(
        &l,
        &[e],
        variable(&[(0.0, 1.0), (0.5, 2.0), (1.0, 1.5)]),
        false,
    )
    .unwrap();
    // Die Kante kann von oben nach unten laufen; das Integral von r² hängt davon nicht ab.
    let want = A * simpson(0.0, 5.0, 2000, |x| hand_law(x / 5.0).powi(2));
    check_ok(&l, &b, want, "konkav");
}

/// Halbes Stadion: Gerade (0,0)–(10,0), Halbkreis um (10, 2), Gerade (10,4)–(0,4), Wand x = 0.
fn half_stadium() -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, 3.0).unwrap()
}

#[test]
fn a_chain_carries_the_law_across_smooth_transitions() {
    // Deckelkante über Gerade, Halbkreis (R 2) und Gerade, beide Enden an der Wand x = 0
    // (senkrecht). Symmetrisches Gesetz (0, 0,5), (0,5, 0,9), (1, 0,5) längs der Kette. Die
    // Kettenlänge misst der Kernel je Kante mit 48 Sehnen (`Seg::length`): Gerade 10,
    // Halbkreis L₄₈ = 96 · R · sin(π/96); die Formel nimmt dieselbe Einteilung.
    let s = half_stadium();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = blend_edges(
        &s,
        &[e],
        variable(&[(0.0, 0.5), (0.5, 0.9), (1.0, 0.5)]),
        true,
    )
    .unwrap();
    assert_eq!(b.edges.len(), 3);
    let law = |f: f64| {
        if f <= 0.5 {
            hermite(f / 0.5, 0.5, (0.5, 0.8), (0.9, 0.0))
        } else {
            hermite((f - 0.5) / 0.5, 0.5, (0.9, 0.0), (0.5, -0.8))
        }
    };
    let big = 2.0;
    let arc48 = 96.0 * big * (PI / 96.0).sin();
    let total = 20.0 + arc48;
    let straight = simpson(0.0, 10.0, 2000, |x| A * law(x / total).powi(2));
    let arc = simpson(0.0, PI, 2000, |phi| {
        let r = law((10.0 + arc48 * phi / PI) / total);
        A * r * r * (big - spandrel_offset() * r)
    });
    check_ok(&s, &b, -(2.0 * straight + arc), "halbes Stadion");
    // Ohne Kette endet die Gerade an der Halbzylinderwand: kein ebener Abschluss (P5), benannt.
    let r = blend_edges(&s, &[e], variable(&[(0.0, 0.5), (1.0, 0.9)]), false);
    assert!(
        matches!(&r, Err(BlendError::Unsupported(m)) if m == planora_kernel::blend::march::OBLIQUE_END),
        "{:?}",
        r.map(|_| ())
    );
}

#[test]
fn closed_chains_need_equal_radii_at_both_ends() {
    // Ganzes Stadion: die Deckelkette ist geschlossen.
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Arc {
                    center: DVec2::new(0.0, 2.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    match blend_edges(&s, &[e], variable(&[(0.0, 0.5), (1.0, 0.8)]), true) {
        Err(BlendError::InvalidParameter(m)) => assert!(m.contains("geschlossenen Kette"), "{m}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    let b = blend_edges(
        &s,
        &[e],
        variable(&[(0.0, 0.5), (0.5, 0.8), (1.0, 0.5)]),
        true,
    )
    .unwrap();
    assert_eq!(b.edges.len(), 4);
    let got = delta_volume(&s, &b.solid, CHORD);
    // Kein fester Kettenanfang in der Formel nötig: Grenzen aus kleinstem und größtem Radius.
    let len = 20.0 + 4.0 * PI;
    assert!(
        got < -A * 0.25 * len * 0.99 && got > -A * 0.64 * len,
        "{got}"
    );
    assert!(plausible(&s, &b, CHORD));
}

/// Würfel 10 minus Zylinder (Mitte `c`, Radius `r`) von z = −1 bis `top`.
fn notched(c: (f64, f64), r: f64, top: f64) -> Solid {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let tool = k()
        .transform(
            &k().make_cylinder(r, top + 1.0).unwrap(),
            &DMat4::from_translation(DVec3::new(c.0, c.1, -1.0)),
        )
        .unwrap();
    k().boolean(&cube, &tool, BoolOp::Difference).unwrap()
}

/// Würfel 10 minus Quader `size` an der Stelle `at`.
fn pocketed(at: DVec3, size: DVec3) -> Solid {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let tool = k()
        .transform(&k().make_box(size).unwrap(), &DMat4::from_translation(at))
        .unwrap();
    k().boolean(&cube, &tool, BoolOp::Difference).unwrap()
}

fn front_edge(s: &Solid) -> Id<Edge> {
    let near = |v: Id<Vertex>, p: DVec3| s.vertices[v].point.distance(p) < 1e-9;
    let (a, b) = (DVec3::new(0.0, 0.0, 10.0), DVec3::new(10.0, 0.0, 10.0));
    s.edges
        .iter()
        .find(|(_, e)| (near(e.start, a) && near(e.end, b)) || (near(e.start, b) && near(e.end, a)))
        .map(|(id, _)| id)
        .unwrap()
}

fn assert_in_area(r: Result<Blended, BlendError>, what: &str) {
    match r {
        Err(BlendError::Unsupported(m)) => {
            assert!(m.contains("Bereich der Rundung"), "{what}: {m}")
        }
        Err(BlendError::TooLarge { face }) => panic!("{what}: zu groß für {face}"),
        Err(e) => panic!("{what}: {e:?}"),
        Ok(b) => panic!("{what}: Ok mit {} Flächen", b.solid.faces.len()),
    }
}

#[test]
fn holes_notches_and_pockets_near_a_skin_are_refused_or_exact() {
    // Gesetz (0, 1,8), (1, 2,2): r(5) = 2.
    let law = [(0.0, 1.8), (1.0, 2.2)];
    let want = -A * simpson(0.0, 10.0, 2000, |x| (1.8 + 0.4 * x / 10.0).powi(2));
    // Sackloch von unten schneidet Wand 1 bis z 9,5 an: die Berührkurve (z ≈ 10 − r) läuft durch
    // die Aussparung (genaue Schnittprüfung mit der NURBS-Berührkurve).
    let s = notched((1.83, 0.30), 0.65, 9.5);
    assert_in_area(
        blend_edges(&s, &[front_edge(&s)], variable(&law), false),
        "Aussparung",
    );
    // Dasselbe Loch nur bis z 6: unter der Berührkurve, das Ergebnis ist exakt.
    let s = notched((1.83, 0.30), 0.65, 6.0);
    let b = blend_edges(&s, &[front_edge(&s)], variable(&law), false).unwrap();
    check_ok(&s, &b, want, "Aussparung unter der Berührkurve");
    // Loch im Deckel im abgetragenen Bereich (y 0,7 … 1,3 < r): benannt.
    let s = notched((5.0, 1.0), 0.3, 12.0);
    assert_in_area(
        blend_edges(&s, &[front_edge(&s)], variable(&law), false),
        "Loch im Deckel",
    );
    // Tasche von unten, die durch die Hautfläche bricht (bei y 0,5 liegt sie bei z ≈ 9,32).
    let s = pocketed(DVec3::new(4.5, 0.3, -1.0), DVec3::new(1.0, 0.4, 10.5));
    assert_in_area(
        blend_edges(&s, &[front_edge(&s)], variable(&law), false),
        "Tasche durch die Hautfläche",
    );
    // Dieselbe Tasche bis z 9: unter der Hautfläche, exakt.
    let s = pocketed(DVec3::new(4.5, 0.3, -1.0), DVec3::new(1.0, 0.4, 10.0));
    let b = blend_edges(&s, &[front_edge(&s)], variable(&law), false).unwrap();
    check_ok(&s, &b, want, "Tasche unter der Hautfläche");
}

#[test]
fn corners_sizes_and_curvature_limits_are_named() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&s, "Deckel", "Wand 1");
    let e2 = edge_between(&s, "Deckel", "Wand 2");
    // Zwei Kanten mit variablem Radius an einer Ecke (P6, Task 17).
    match blend_edges(&s, &[e1, e2], variable(&[(0.0, 1.0), (1.0, 1.5)]), false) {
        Err(BlendError::Corner(m)) => assert_eq!(m, VARIABLE_CORNER),
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Hautflächen verschweißen mit der Knotentoleranz: Mindestmaß 2e-3 mm.
    match blend_edges(&s, &[e1], variable(&[(0.0, 1.0), (1.0, 1e-3)]), false) {
        Err(BlendError::InvalidParameter(m)) => {
            assert!(m.contains("ist zu klein") && m.contains("0.002"), "{m}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
    let b = blend_edges(&s, &[e1], variable(&[(0.0, 2e-3), (1.0, 0.5)]), false).unwrap();
    validate(&b.solid).unwrap();
    // Zylinder R 5: Bei r 3 kommt der Kugelmittelpunkt (Abstand 2) der Achse näher als r.
    let c = k().make_cylinder(5.0, 8.0).unwrap();
    let rim = edge_between(&c, "Wand 1", "Deckel");
    match blend_edges(
        &c,
        &[rim],
        variable(&[(0.0, 3.0), (0.5, 2.0), (1.0, 3.0)]),
        false,
    ) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("Drehachse"), "{m}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Zu groß für die Fläche: benannt mit Flächennamen.
    match blend_edges(&s, &[e1], variable(&[(0.0, 1.0), (1.0, 11.0)]), false) {
        Err(BlendError::TooLarge { face }) => {
            assert!(face == "Deckel" || face == "Wand 1", "{face}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn variable_results_are_deterministic_and_constant_requests_stay_analytic() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let kind = variable(&[(0.0, 1.0), (0.3, 1.7), (1.0, 1.2)]);
    let x = blend_edges(&s, &[e], kind.clone(), false).unwrap();
    let y = blend_edges(&s, &[e], kind, false).unwrap();
    assert_eq!(x.volume_change.to_bits(), y.volume_change.to_bits());
    assert_eq!(x.solid.faces.len(), y.solid.faces.len());
    // Konstanter Radius: weiter der analytische Zylinder (Stufe 1), keine Hautfläche.
    let c = blend_edges(&s, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    assert!(
        c.solid
            .faces
            .iter()
            .all(|(_, f)| !matches!(f.surface, Surface::Nurbs(_)))
    );
    assert!(
        c.solid
            .faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
    );
}
