//! Nächster Punkt auf geschlossenen Kreisen und Ellipsen nahe der Naht (`Seg::closest`) und der
//! Schnitt durch eine Bohrung knapp neben ihrer Naht.
//!
//! Ein Vollkreis als eine Kante hat Anfang = Ende. Liegt der gesuchte Punkt kurz vor dem Ende,
//! sind Stützstelle 0 und Stützstelle n gleich nah; früher wurde dann nur am Anfang gesucht und
//! die Naht selbst als nächster Punkt geliefert. Freiformkurven bleiben bewusst unverändert.

use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Schräg liegendes Koordinatensystem, damit nichts zufällig auf Weltachsen fällt.
fn tilted() -> Frame {
    let m = DMat4::from_translation(DVec3::new(3.0, -2.0, 7.0))
        * DMat4::from_rotation_z(0.4)
        * DMat4::from_rotation_x(0.7);
    Frame::WORLD.transformed(&m)
}

fn circle(r: f64) -> Curve {
    Curve::Circle {
        frame: tilted(),
        radius: r,
    }
}

fn ellipse(a: f64, b: f64) -> Curve {
    Curve::Ellipse {
        frame: tilted(),
        major: a,
        minor: b,
    }
}

/// Abstände (Bogenmaß) vor dem Ende bzw. hinter dem Anfang bis gut 4° (eine Stützstelle sind 3,75°).
const NEAR: [f64; 9] = [1e-9, 1e-6, 1e-4, 1e-3, 0.01, 0.03, 0.05, 0.065, 0.075];

/// Prüft den nächsten Punkt gegen den erwarteten Kurvenparameter `t_want` (Abstand `h`).
fn check(seg: &Seg, p: DVec3, t_want: f64, h: f64, what: &str) {
    let want = seg.curve.point(t_want);
    let (f, d) = seg.closest(p);
    assert!((0.0..=1.0).contains(&f), "{what}: f = {f}");
    assert!((d - h).abs() < 1e-9, "{what}: Abstand {d}, erwartet {h}");
    // Neben der Kurve ist der Abstand am Minimum flach: die Lage ist dort nur auf etwa
    // h · 1e-6 bestimmt (der Abstand selbst bleibt auf 1e-9 genau). Auf der Kurve: 1e-9.
    let tol_point = 1e-9 + h * 1e-6;
    let got = seg.at(f);
    assert!(
        got.distance(want) < tol_point,
        "{what}: Punkt {got:?}, erwartet {want:?} (Abweichung {:e})",
        got.distance(want)
    );
}

/// Punkt bei Kurvenparameter `t`, `h` senkrecht aus der Kurvenebene (nächster Punkt bleibt bei `t`).
fn lifted(c: &Curve, t: f64, h: f64) -> DVec3 {
    c.point(t) + tilted().z * h
}

/// Vollkreise und Vollellipsen in verschiedenen Lagen des Parameterbereichs (auch rückwärts).
fn closed_segs() -> Vec<Seg> {
    let mut out = Vec::new();
    for c in [
        circle(10.0),
        circle(4.0),
        circle(0.5),
        ellipse(10.0, 4.0),
        ellipse(3.0, 2.5),
    ] {
        for (t0, t1) in [(0.0, TAU), (1.3, 1.3 + TAU), (-2.0, -2.0 + TAU), (TAU, 0.0)] {
            out.push(Seg::new(c.clone(), t0, t1));
        }
    }
    out
}

#[test]
fn closed_circle_and_ellipse_find_points_just_before_the_end() {
    for seg in closed_segs() {
        assert!(seg.start().distance(seg.end()) < 1e-9);
        for delta in NEAR {
            for h in [0.0, 1e-3, 0.01] {
                let t = seg.t1 - (seg.t1 - seg.t0).signum() * delta;
                let p = lifted(&seg.curve, t, h);
                check(
                    &seg,
                    p,
                    t,
                    h,
                    &format!("{seg:?} vor dem Ende δ {delta} h {h}"),
                );
            }
        }
    }
}

#[test]
fn closed_circle_and_ellipse_find_points_just_after_the_start() {
    for seg in closed_segs() {
        for delta in NEAR {
            for h in [0.0, 1e-3, 0.01] {
                let t = seg.t0 + (seg.t1 - seg.t0).signum() * delta;
                let p = lifted(&seg.curve, t, h);
                check(
                    &seg,
                    p,
                    t,
                    h,
                    &format!("{seg:?} hinter dem Anfang δ {delta} h {h}"),
                );
            }
        }
    }
}

#[test]
fn closed_circle_points_off_the_curve_in_its_plane() {
    // Punkte innen und außen neben dem Kreis, knapp vor der Naht: der nächste Punkt liegt
    // radial, der Abstand ist |Radius − Abstand zum Mittelpunkt|.
    let c = circle(10.0);
    let fr = tilted();
    let seg = Seg::new(c.clone(), 0.0, TAU);
    for delta in [1e-4, 0.01, 0.05, 0.065] {
        for rho in [9.99, 9.9, 10.01, 10.1] {
            let t = TAU - delta;
            let p = fr.origin + (fr.x * t.cos() + fr.y * t.sin()) * rho;
            let (f, d) = seg.closest(p);
            assert!(
                (d - (rho - 10.0_f64).abs()).abs() < 1e-9,
                "δ {delta} ρ {rho}: {d}"
            );
            assert!(seg.at(f).distance(c.point(t)) < 1e-6, "δ {delta} ρ {rho}");
        }
    }
}

#[test]
fn seam_point_itself_and_far_points_stay_as_before() {
    for seg in closed_segs() {
        // Genau auf der Naht: Abstand 0 (Anfang oder Ende, beides derselbe Punkt).
        let (f, d) = seg.closest(seg.start());
        assert!(d < 1e-12 && (f == 0.0 || f == 1.0 || seg.at(f).distance(seg.start()) < 1e-9));
        // Gegenüber der Naht: wie bisher mitten im Stück.
        let mid = seg.param(0.5);
        let p = lifted(&seg.curve, mid, 0.0);
        let (f, d) = seg.closest(p);
        assert!(d < 1e-9 && (f - 0.5).abs() < 1e-9, "{seg:?}: f {f} d {d}");
    }
}

#[test]
fn open_arcs_are_unchanged() {
    // Offene Bögen: Anfang ≠ Ende. Punkte vor dem Ende und in der Lücke liefern wie bisher den
    // richtigen Punkt; in der Lücke ist es das nähere Ende.
    let c = circle(10.0);
    for (t0, t1) in [
        (0.0, TAU - 0.2),
        (0.0, 1.0),
        (TAU - 0.2, 0.0),
        (0.5, 0.5 + PI),
    ] {
        let seg = Seg::new(c.clone(), t0, t1);
        assert!(seg.start().distance(seg.end()) > 1e-3);
        for delta in NEAR {
            let t = t1 - (t1 - t0).signum() * delta;
            check(
                &seg,
                lifted(&c, t, 0.0),
                t,
                0.0,
                &format!("{seg:?} δ {delta}"),
            );
            let t = t0 + (t1 - t0).signum() * delta;
            check(
                &seg,
                lifted(&c, t, 0.0),
                t,
                0.0,
                &format!("{seg:?} δ {delta}"),
            );
        }
    }
    // Lücke des fast geschlossenen Bogens 0 … 2π − 0,2: näher am Anfang → f = 0, näher am Ende → f = 1.
    let seg = Seg::new(c.clone(), 0.0, TAU - 0.2);
    let (f, d) = seg.closest(c.point(-0.05));
    assert!(f.abs() < 1e-9, "{f}");
    assert!((d - c.point(0.0).distance(c.point(-0.05))).abs() < 1e-9);
    let (f, d) = seg.closest(c.point(-0.15));
    assert!((f - 1.0).abs() < 1e-9, "{f}");
    assert!((d - c.point(-0.2).distance(c.point(-0.15))).abs() < 1e-9);
}

fn nurbs_circle(r: f64) -> Curve {
    let fr = tilted();
    Curve::nurbs(NurbsCurve::elliptic_arc(fr.origin, fr.x * r, fr.y * r, 0.0, TAU).unwrap())
}

#[test]
fn closed_freeform_keeps_its_old_search() {
    // Geschlossene Freiformkurve: die Suche bleibt bewusst wie bisher (nur der Abschnitt an der
    // nächsten Stützstelle). Ein Punkt kurz vor dem Ende landet daher weiter an der Naht, wenn
    // Stützstelle 0 gewinnt; Punkte hinter dem Anfang werden genau gefunden.
    let c = nurbs_circle(10.0);
    let (a, b) = c.domain().unwrap();
    let seg = Seg::new(c.clone(), a, b);
    assert!(seg.start().distance(seg.end()) < 1e-9);
    let seam = seg.start();
    for delta in [1e-3, 0.01, 0.02] {
        let p = c.point(b - delta * (b - a) / TAU);
        let (f, d) = seg.closest(p);
        assert!(f < 0.5, "δ {delta}: f {f}");
        assert!(
            (d - p.distance(seam)).abs() < 1e-9,
            "δ {delta}: Abstand {d}, Naht {}",
            p.distance(seam)
        );
        let p = c.point(a + delta * (b - a) / TAU);
        let (_, d) = seg.closest(p);
        assert!(d < 1e-9, "δ {delta}: {d}");
    }
}

/// Fläche der ebenen Fläche des Körpers in der Ebene `y = y0` (Schleifen abgetastet, x/z-Ebene).
fn face_area_at_y(s: &Solid, y0: f64) -> f64 {
    let pts_of = |lp: &Loop| -> Vec<DVec3> {
        let mut out = Vec::new();
        for he in &lp.half_edges {
            let e = &s.edges[he.edge];
            let m = if matches!(e.curve, Curve::Line { .. }) {
                2
            } else {
                512
            };
            let mut pts: Vec<DVec3> = (0..m)
                .map(|i| {
                    e.curve
                        .point(e.t0 + (e.t1 - e.t0) * i as f64 / (m - 1) as f64)
                })
                .collect();
            if !he.forward {
                pts.reverse();
            }
            pts.pop();
            out.extend(pts);
        }
        out
    };
    let shoelace = |pts: &[DVec3]| -> f64 {
        let n = pts.len();
        ((0..n)
            .map(|i| {
                let (p, q) = (pts[i], pts[(i + 1) % n]);
                p.x * q.z - q.x * p.z
            })
            .sum::<f64>()
            / 2.0)
            .abs()
    };
    let mut total = 0.0;
    let mut found = 0;
    for (_, f) in s.faces.iter() {
        let outer = pts_of(&f.outer);
        if !outer.iter().all(|p| (p.y - y0).abs() < 1e-9) {
            continue;
        }
        found += 1;
        total += shoelace(&outer) - f.inner.iter().map(|l| shoelace(&pts_of(l))).sum::<f64>();
    }
    assert!(found > 0, "keine Fläche bei y = {y0}");
    total
}

#[test]
fn half_space_cut_just_beside_a_bore_axis_succeeds() {
    // Quader 100 × 60 × 40 mit Bohrung r 10 längs z (Achse x = 50, y = 30). Die Naht der Bohrung
    // liegt bei +x in der Ebene y = 30; die Schnittebenen y = 30 + d treffen den Bohrungskreis
    // knapp neben der Naht. Weggenommen wird alles unterhalb der Ebene.
    let block = k().make_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let drill = k()
        .transform(
            &k().make_cylinder(10.0, 44.0).unwrap(),
            &DMat4::from_translation(DVec3::new(50.0, 30.0, -2.0)),
        )
        .unwrap();
    let body = k().boolean(&block, &drill, BoolOp::Difference).unwrap();
    for d in [0.1, 0.01, 1e-4, 0.5, -0.1, -0.01, -1e-4, -0.5] {
        let y0 = 30.0 + d;
        let cutter = k()
            .transform(
                &k().make_box(DVec3::new(200.0, 100.0, 100.0)).unwrap(),
                &DMat4::from_translation(DVec3::new(-50.0, y0 - 100.0, -30.0)),
            )
            .unwrap();
        let out = k()
            .boolean(&body, &cutter, BoolOp::Difference)
            .unwrap_or_else(|e| panic!("d = {d}: {e}"));
        validate(&out).unwrap_or_else(|e| panic!("d = {d}: {e}"));
        let want = 100.0 * 40.0 - 2.0 * (100.0 - d * d).sqrt() * 40.0;
        let got = face_area_at_y(&out, y0);
        assert!(
            (got - want).abs() < 1e-6,
            "d = {d}: Fläche {got}, erwartet {want}"
        );
    }
}
