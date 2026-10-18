//! Stufe 9: Blattmodell und Ausgabekurven (Linien, Bögen, Ellipsen, Splines).
//! Alle Sollwerte werden hier von Hand gerechnet (eigene Achsentabelle, eigene Projektion,
//! eigene Singulärwerte), nicht aus der Bibliothek übernommen.

use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::out2d::{Out2d, piece_to_outs};
use planora_drawing::sheet::{SheetTransform, build_sheet, view_center};
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{Basis, Piece, ViewId, ViewKind, ViewLines, Visibility};
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use std::collections::BTreeMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------
// Eigene Hilfen
// ---------------------------------------------------------------------------------------------

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn cross(a: DVec3, b: DVec3) -> DVec3 {
    v3(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Blattachsen `(ex, ey)` der Ansicht, von Hand: Blick `d`, „oben“ `ey`, `ex = d × ey`.
fn axes(kind: ViewKind) -> (DVec3, DVec3) {
    let (d, ey) = match kind {
        ViewKind::Front => (v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Back => (v3(0.0, -1.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Left => (v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Right => (v3(-1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Top => (v3(0.0, 0.0, -1.0), v3(0.0, 1.0, 0.0)),
        ViewKind::Bottom => (v3(0.0, 0.0, 1.0), v3(0.0, -1.0, 0.0)),
        ViewKind::Isometric => {
            // Blick (−1, 1, −1)/√3; „oben“ = Z-Anteil senkrecht zum Blick: (−1, 1, 2)/√6.
            let (s3, s6) = (3f64.sqrt(), 6f64.sqrt());
            (
                v3(-1.0 / s3, 1.0 / s3, -1.0 / s3),
                v3(-1.0 / s6, 1.0 / s6, 2.0 / s6),
            )
        }
    };
    (cross(d, ey), ey)
}

/// Bild eines Raumpunkts in der Ansicht (eigene Projektion).
fn img(p: DVec3, kind: ViewKind) -> DVec2 {
    let (ex, ey) = axes(kind);
    DVec2::new(p.dot(ex), p.dot(ey))
}

fn frame(origin: DVec3, x: DVec3, y: DVec3) -> Frame {
    Frame {
        origin,
        x,
        y,
        z: cross(x, y),
    }
}

fn stroke(curve: Curve, t0: f64, t1: f64) -> Stroke {
    Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

fn circle(f: Frame, r: f64) -> Curve {
    Curve::Circle {
        frame: f,
        radius: r,
    }
}

fn xf_id() -> SheetTransform {
    SheetTransform {
        scale: 1.0,
        tx: 0.0,
        ty: 0.0,
        bbox_center: DVec2::ZERO,
    }
}

fn out(s: &Stroke, kind: ViewKind, t0: f64, t1: f64) -> Out2d {
    let mut v = piece_to_outs(s, &Basis::of(kind), t0, t1, &xf_id());
    assert_eq!(v.len(), 1, "genau eine Kurve erwartet: {v:?}");
    v.remove(0)
}

/// Winkelunterschied auf `(−π, π]` gefaltet.
fn wrap(a: f64) -> f64 {
    a - TAU * (a / TAU).round()
}

fn close(a: DVec2, b: DVec2, tol: f64) -> bool {
    (a - b).length() <= tol
}

// ---------------------------------------------------------------------------------------------
// (a) Kreis von oben
// ---------------------------------------------------------------------------------------------

#[test]
fn circle_in_top_view_becomes_arc_with_full_turn() {
    // Kreis r = 7 um (3, 4, 5) in der XY-Ebene, von oben: Mittelpunkt (3, 4), Radius 7.
    let s = stroke(
        circle(
            frame(v3(3.0, 4.0, 5.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)),
            7.0,
        ),
        0.0,
        TAU,
    );
    let o = out(&s, ViewKind::Top, 0.0, TAU);
    let Out2d::Arc { c, r, start, end } = o.clone() else {
        panic!("Arc erwartet, erhalten {o:?}")
    };
    assert!(close(c, DVec2::new(3.0, 4.0), 1e-12), "{c:?}");
    assert!((r - 7.0).abs() < 1e-12, "{r}");
    assert!(start < end);
    assert!((end - (start + TAU)).abs() < 1e-12, "{start} {end}");
    // Der Bogen beginnt bei P(0) = (10, 4) und läuft gegen den Uhrzeigersinn.
    assert!(close(o.point(start), DVec2::new(10.0, 4.0), 1e-9));
    for i in 0..50 {
        let t = TAU * i as f64 / 50.0;
        let want = DVec2::new(3.0 + 7.0 * t.cos(), 4.0 + 7.0 * t.sin());
        assert!(close(o.point(start + t), want, 1e-9), "t = {t}");
    }
    assert!((o.length() - TAU * 7.0).abs() < 1e-9);
}

#[test]
fn partial_arc_in_top_view_keeps_its_angles() {
    let s = stroke(
        circle(
            frame(v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)),
            2.0,
        ),
        0.4,
        2.2,
    );
    let Out2d::Arc { start, end, .. } = out(&s, ViewKind::Top, 0.4, 2.2) else {
        panic!("Arc erwartet")
    };
    assert!(wrap(start - 0.4).abs() < 1e-12, "{start}");
    assert!((end - start - 1.8).abs() < 1e-12);
}

// ---------------------------------------------------------------------------------------------
// (b) Kreis von vorne = Strecke, auch gefaltet
// ---------------------------------------------------------------------------------------------

fn flat_circle() -> Stroke {
    // Kreis r = 4 um (2, 3, 1) in der XY-Ebene; in der Vorderansicht (ex = X, ey = Z) kantenparallel.
    stroke(
        circle(
            frame(v3(2.0, 3.0, 1.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)),
            4.0,
        ),
        0.0,
        TAU,
    )
}

fn line_ends(o: &Out2d) -> (DVec2, DVec2) {
    let Out2d::Line { a, b } = o else {
        panic!("Line erwartet, erhalten {o:?}")
    };
    (a.min(*b), a.max(*b))
}

#[test]
fn circle_in_front_view_becomes_line_of_diameter_length() {
    let s = flat_circle();
    let o = out(&s, ViewKind::Front, 0.0, TAU);
    assert!((o.length() - 8.0).abs() < 1e-9, "{}", o.length());
    let (lo, hi) = line_ends(&o);
    assert!(close(lo, DVec2::new(-2.0, 1.0), 1e-9), "{lo:?}");
    assert!(close(hi, DVec2::new(6.0, 1.0), 1e-9), "{hi:?}");
}

#[test]
fn edge_on_arc_that_folds_back_uses_the_exact_extremes() {
    let s = flat_circle();
    // Gefaltet um t = 0: x = 2 + 4 cos t läuft bis 6 hinaus und zurück.
    let o = out(&s, ViewKind::Front, -0.5, 0.5);
    let (lo, hi) = line_ends(&o);
    assert!(
        close(lo, DVec2::new(2.0 + 4.0 * 0.5f64.cos(), 1.0), 1e-9),
        "{lo:?}"
    );
    assert!(close(hi, DVec2::new(6.0, 1.0), 1e-9), "{hi:?}");
    // Gefaltet um t = π: bis x = −2.
    let o = out(&s, ViewKind::Front, 2.5, 3.8);
    let (lo, hi) = line_ends(&o);
    assert!(close(lo, DVec2::new(-2.0, 1.0), 1e-9), "{lo:?}");
    let want = (2.0 + 4.0 * 2.5f64.cos()).max(2.0 + 4.0 * 3.8f64.cos());
    assert!(close(hi, DVec2::new(want, 1.0), 1e-9), "{hi:?}");
    // Beide Umkehrpunkte im Bereich: ganze Breite.
    let o = out(&s, ViewKind::Front, -0.5, 3.8);
    let (lo, hi) = line_ends(&o);
    assert!(close(lo, DVec2::new(-2.0, 1.0), 1e-9));
    assert!(close(hi, DVec2::new(6.0, 1.0), 1e-9));
    // Nicht gefaltet: genau die beiden Endpunkte.
    let o = out(&s, ViewKind::Front, 0.5, 1.0);
    let (a, b) = match &o {
        Out2d::Line { a, b } => (*a, *b),
        _ => panic!("Line erwartet"),
    };
    assert!(close(a, DVec2::new(2.0 + 4.0 * 0.5f64.cos(), 1.0), 1e-9));
    assert!(close(b, DVec2::new(2.0 + 4.0 * 1.0f64.cos(), 1.0), 1e-9));
}

// ---------------------------------------------------------------------------------------------
// (c) Geneigter Kreis = Ellipse
// ---------------------------------------------------------------------------------------------

/// Singulärwerte der 2×2-Matrix `[A B]`, eigene Rechnung über die Eigenwerte von `M·Mᵀ`.
fn singular_values(a: DVec2, b: DVec2) -> (f64, f64) {
    let t = a.length_squared() + b.length_squared();
    let det = a.x * b.y - a.y * b.x;
    let disc = (t * t - 4.0 * det * det).max(0.0).sqrt();
    (((t + disc) / 2.0).sqrt(), ((t - disc) / 2.0).sqrt())
}

#[test]
fn tilted_circle_is_ellipse_with_expected_axes() {
    // Kreis r = 5 um den Ursprung in der XY-Ebene, isometrische Ansicht: d·z = −1/√3, daher
    // σ1 = r, σ2 = r/√3; die Hauptachse ist das Bild von (1,1,0)/√2 (senkrecht zum Blick in
    // der Ebene) und liegt wegen ex = (1,1,0)/√2 in Richtung der x-Achse des Blatts.
    let r = 5.0;
    let s = stroke(
        circle(frame(DVec3::ZERO, v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)), r),
        0.0,
        TAU,
    );
    let o = out(&s, ViewKind::Isometric, 0.0, TAU);
    let Out2d::Ellipse {
        c,
        major,
        ratio,
        start,
        end,
    } = o.clone()
    else {
        panic!("Ellipse erwartet, erhalten {o:?}")
    };
    let inv3 = 1.0 / 3f64.sqrt();
    assert!(close(c, DVec2::ZERO, 1e-12));
    assert!((major.length() - r).abs() < 1e-9, "σ1 = {}", major.length());
    assert!((ratio - inv3).abs() < 1e-9, "ratio = {ratio}");
    assert!((major.length() * ratio - r * inv3).abs() < 1e-9);
    // Hauptachse in x-Richtung (±), Nebenachse (senkrecht dazu) in y-Richtung.
    assert!(major.y.abs() < 1e-9 * r, "{major:?}");
    // Eigene Singulärwerte aus der Matrix der projizierten Halbmesser.
    let a = img(v3(r, 0.0, 0.0), ViewKind::Isometric);
    let b = img(v3(0.0, r, 0.0), ViewKind::Isometric);
    let (s1, s2) = singular_values(a, b);
    assert!(
        (s1 - r).abs() < 1e-9 && (s2 - r * inv3).abs() < 1e-9,
        "{s1} {s2}"
    );
    // Die Achsen stehen senkrecht aufeinander: die Nebenachse ist `ratio·|major|` lang und
    // steht auf `major` senkrecht (Nebenvektor = Drehung um 90°).
    let minor = DVec2::new(-major.y, major.x) * ratio;
    assert!(major.dot(minor).abs() < 1e-9);
    // Der Umlauf ist nicht gespiegelt: Parameter start + (t − 0) bildet P(t) ab.
    assert!(start < end && (end - start - TAU).abs() < 1e-12);
    for i in 0..50 {
        let t = TAU * i as f64 / 50.0 + 0.013;
        let p = v3(r * t.cos(), r * t.sin(), 0.0);
        let want = img(p, ViewKind::Isometric);
        assert!(close(o.point(start + t), want, 1e-9), "t = {t}");
    }
    // Umfang der Ellipse: eigene Streckenzug-Rechnung über das Bild des Kreises.
    let n = 200_000;
    let mut len = 0.0;
    let mut prev = img(v3(r, 0.0, 0.0), ViewKind::Isometric);
    for i in 1..=n {
        let t = TAU * i as f64 / n as f64;
        let p = img(v3(r * t.cos(), r * t.sin(), 0.0), ViewKind::Isometric);
        len += p.distance(prev);
        prev = p;
    }
    assert!((o.length() - len).abs() < 1e-6, "{} {}", o.length(), len);
}

/// Allgemeine Ellipse in schräger Lage, gespiegelte (Back) und nicht gespiegelte (Front) Ansicht.
fn tilted_ellipse() -> Stroke {
    let c = 25f64.to_radians().cos();
    let sn = 25f64.to_radians().sin();
    let f = frame(
        v3(1.0, -2.0, 3.0),
        v3(0.6, 0.8, 0.0),
        v3(-0.8 * c, 0.6 * c, sn),
    );
    stroke(
        Curve::Ellipse {
            frame: f,
            major: 6.0,
            minor: 2.5,
        },
        0.0,
        TAU,
    )
}

fn ellipse_world(t: f64) -> DVec3 {
    let c = 25f64.to_radians().cos();
    let sn = 25f64.to_radians().sin();
    let (x, y) = (v3(0.6, 0.8, 0.0), v3(-0.8 * c, 0.6 * c, sn));
    v3(1.0, -2.0, 3.0) + x * (6.0 * t.cos()) + y * (2.5 * t.sin())
}

#[test]
fn general_ellipse_matches_projected_points_in_all_views() {
    let s = tilted_ellipse();
    for kind in ViewKind::ALL {
        let (t0, t1) = (0.3, 4.9);
        let o = out(&s, kind, t0, t1);
        let (ex, ey) = axes(kind);
        let a = DVec2::new(
            (v3(0.6, 0.8, 0.0) * 6.0).dot(ex),
            (v3(0.6, 0.8, 0.0) * 6.0).dot(ey),
        );
        let c = 25f64.to_radians().cos();
        let sn = 25f64.to_radians().sin();
        let yv = v3(-0.8 * c, 0.6 * c, sn) * 2.5;
        let b = DVec2::new(yv.dot(ex), yv.dot(ey));
        let det = a.x * b.y - a.y * b.x;
        let (s1, s2) = singular_values(a, b);
        match &o {
            Out2d::Ellipse {
                major,
                ratio,
                start,
                end,
                ..
            } => {
                assert!((major.length() - s1).abs() < 1e-9, "{kind:?}");
                assert!((ratio - s2 / s1).abs() < 1e-9, "{kind:?}");
                assert!(
                    start < end && (end - start - (t1 - t0)).abs() < 1e-12,
                    "{kind:?}"
                );
                for i in 0..=50 {
                    let t = t0 + (t1 - t0) * i as f64 / 50.0;
                    let want = img(ellipse_world(t), kind);
                    // Nicht gespiegelt: start + (t − t0); gespiegelt: end − (t − t0).
                    let u = if det > 0.0 {
                        start + (t - t0)
                    } else {
                        end - (t - t0)
                    };
                    assert!(close(o.point(u), want, 1e-9), "{kind:?} t = {t}");
                }
            }
            Out2d::Line { .. } => assert!(s2 / s1 <= 1e-9, "{kind:?}"),
            other => panic!("{kind:?}: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (d) Gespiegelte Ansichten
// ---------------------------------------------------------------------------------------------

#[test]
fn reflected_view_keeps_counter_clockwise_arcs() {
    // (Ansicht, Rahmen x, Rahmen y, Bildwinkel = k − t, Erwartung von Hand):
    //   Back: Kreis in der XZ-Ebene → Bild (−cos t, sin t) = Winkel π − t,
    //   Bottom: Kreis in der XY-Ebene → Bild (cos t, −sin t) = Winkel −t,
    //   Top mit vertauschtem Rahmen (x = Y, y = X) → Bild (sin t, cos t) = Winkel π/2 − t.
    let cases = [
        (ViewKind::Back, v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0), PI),
        (ViewKind::Bottom, v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), 0.0),
        (
            ViewKind::Top,
            v3(0.0, 1.0, 0.0),
            v3(1.0, 0.0, 0.0),
            PI / 2.0,
        ),
    ];
    let (t0, t1) = (0.3, 1.9);
    for (kind, fx, fy, k) in cases {
        let f = frame(v3(1.0, 2.0, 3.0), fx, fy);
        let s = stroke(circle(f, 5.0), t0, t1);
        let o = out(&s, kind, t0, t1);
        let Out2d::Arc { c, r, start, end } = o.clone() else {
            panic!("{kind:?}: Arc erwartet, erhalten {o:?}")
        };
        assert!(start < end, "{kind:?}: start {start} end {end}");
        assert!((end - start - (t1 - t0)).abs() < 1e-12, "{kind:?}");
        assert!(end - start <= TAU);
        assert!((r - 5.0).abs() < 1e-12);
        assert!(close(c, img(v3(1.0, 2.0, 3.0), kind), 1e-12), "{kind:?}");
        // Der Bogen läuft gegen den Uhrzeigersinn von Bildwinkel k − t1 bis k − t0.
        assert!(
            wrap(start - (k - t1)).abs() < 1e-12,
            "{kind:?}: start {start}"
        );
        assert!(wrap(end - (k - t0)).abs() < 1e-12, "{kind:?}: end {end}");
        // Anfangs-, End- und Mittelpunkt des gezeichneten Stücks stimmen mit dem Bild des
        // 3D-Kreises überein (die Mitte unterscheidet den richtigen vom Ergänzungsbogen).
        let world = |t: f64| f.origin + (f.x * t.cos() + f.y * t.sin()) * 5.0;
        assert!(
            close(o.point(start), img(world(t1), kind), 1e-9),
            "{kind:?}"
        );
        assert!(close(o.point(end), img(world(t0), kind), 1e-9), "{kind:?}");
        assert!(
            close(
                o.point(0.5 * (start + end)),
                img(world(0.5 * (t0 + t1)), kind),
                1e-9
            ),
            "{kind:?}"
        );
    }
}

#[test]
fn full_circle_in_reflected_view_is_a_full_turn() {
    let f = frame(DVec3::ZERO, v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0));
    let s = stroke(circle(f, 3.0), 0.0, TAU);
    let Out2d::Arc { start, end, .. } = out(&s, ViewKind::Back, 0.0, TAU) else {
        panic!("Arc erwartet")
    };
    assert!(start < end && (end - start - TAU).abs() < 1e-12);
}

// ---------------------------------------------------------------------------------------------
// (e) Splines
// ---------------------------------------------------------------------------------------------

fn cubic_nurbs() -> NurbsCurve {
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0],
        vec![
            v3(0.0, 1.0, 0.0),
            v3(2.0, 4.0, 1.0),
            v3(5.0, -2.0, 3.0),
            v3(7.0, 3.0, -1.0),
            v3(9.0, 0.0, 2.0),
            v3(12.0, 5.0, 4.0),
        ],
        vec![1.0; 6],
    )
    .unwrap()
}

fn rational_nurbs() -> NurbsCurve {
    NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 0.4, 0.7, 1.0, 1.0, 1.0],
        vec![
            v3(0.0, 0.0, 0.0),
            v3(3.0, 5.0, 1.0),
            v3(6.0, -1.0, 2.0),
            v3(9.0, 4.0, 0.0),
            v3(12.0, 1.0, 3.0),
        ],
        vec![1.0, 2.0, 0.5, 3.0, 1.0],
    )
    .unwrap()
}

fn check_spline_cut(c: &NurbsCurve, kind: ViewKind, t0: f64, t1: f64) {
    let s = stroke(Curve::Nurbs(Arc::new(c.clone())), t0, t1);
    let o = out(&s, kind, t0, t1);
    let Out2d::Spline {
        degree,
        knots,
        ctrl,
        weights,
    } = &o
    else {
        panic!("Spline erwartet, erhalten {o:?}")
    };
    let p = c.degree();
    assert_eq!(*degree, p);
    assert_eq!(knots.len(), ctrl.len() + p + 1);
    assert_eq!(weights.len(), ctrl.len());
    // Offener Knotenvektor mit Randvielfachheit p + 1, Bereich genau [t0, t1].
    assert!(knots[..=p].iter().all(|&k| k == t0), "{knots:?}");
    assert!(
        knots[knots.len() - p - 1..].iter().all(|&k| k == t1),
        "{knots:?}"
    );
    assert!(
        knots[p + 1] > t0 && knots[knots.len() - p - 2] < t1,
        "{knots:?}"
    );
    assert!(knots.windows(2).all(|w| w[0] <= w[1]));
    let mut worst: f64 = 0.0;
    for i in 0..100 {
        let t = t0 + (t1 - t0) * i as f64 / 99.0;
        let want = img(c.point(t), kind);
        worst = worst.max(o.point(t).distance(want));
    }
    assert!(worst < 1e-9, "größte Abweichung {worst}");
}

#[test]
fn spline_piece_is_cut_exactly() {
    let c = cubic_nurbs();
    for (t0, t1) in [
        (0.15, 0.85),
        (0.3, 0.6),
        (0.0, 0.45),
        (0.2, 1.0),
        (0.0, 1.0),
        (0.31, 0.32),
    ] {
        for kind in [ViewKind::Front, ViewKind::Isometric, ViewKind::Back] {
            check_spline_cut(&c, kind, t0, t1);
        }
    }
}

#[test]
fn rational_spline_piece_is_cut_exactly() {
    let c = rational_nurbs();
    for (t0, t1) in [(0.1, 0.9), (0.4, 0.7), (0.0, 0.55), (0.65, 1.0), (0.0, 1.0)] {
        for kind in [ViewKind::Top, ViewKind::Isometric] {
            check_spline_cut(&c, kind, t0, t1);
        }
    }
}

#[test]
fn spline_weights_and_bounds_are_consistent() {
    // Ein Viertelkreis als rationale Kurve (Grad 2, Gewicht √2/2 in der Mitte): Länge π/2·r.
    let r = 4.0;
    let w = 0.5f64.sqrt();
    let c = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![v3(r, 0.0, 0.0), v3(r, r, 0.0), v3(0.0, r, 0.0)],
        vec![1.0, w, 1.0],
    )
    .unwrap();
    let s = stroke(Curve::Nurbs(Arc::new(c)), 0.0, 1.0);
    let o = out(&s, ViewKind::Top, 0.0, 1.0);
    assert!((o.length() - PI / 2.0 * r).abs() < 1e-9, "{}", o.length());
    let (lo, hi) = o.bounds();
    assert!(close(lo, DVec2::new(0.0, 0.0), 1e-9) && close(hi, DVec2::new(r, r), 1e-9));
    // Teilstück 0.2 … 0.7: Länge = r · Winkel; Bogen vom Winkel φ(0.2) bis φ(0.7) (aus den
    // Punkten, eigene Winkelrechnung).
    let p = |t: f64| {
        let b0 = (1.0 - t) * (1.0 - t);
        let b1 = 2.0 * t * (1.0 - t) * w;
        let b2 = t * t;
        let den = b0 + b1 + b2;
        DVec2::new((b0 * r + b1 * r) / den, (b1 * r + b2 * r) / den)
    };
    let o2 = out(&s, ViewKind::Top, 0.2, 0.7);
    let ang = |q: DVec2| q.y.atan2(q.x);
    let want = r * (ang(p(0.7)) - ang(p(0.2)));
    assert!(
        (o2.length() - want).abs() < 1e-9,
        "{} {}",
        o2.length(),
        want
    );
}

// ---------------------------------------------------------------------------------------------
// Ausgabekurven: Hüllbox
// ---------------------------------------------------------------------------------------------

#[test]
fn bounds_of_arcs_ellipses_and_splines_match_dense_sampling() {
    let dense = |o: &Out2d, t0: f64, t1: f64| {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for i in 0..=100_000 {
            let p = o.point(t0 + (t1 - t0) * i as f64 / 100_000.0);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        (lo, hi)
    };
    let arc = Out2d::Arc {
        c: DVec2::new(1.0, 2.0),
        r: 3.0,
        start: 0.7,
        end: 4.0,
    };
    let ell = Out2d::Ellipse {
        c: DVec2::new(-1.0, 0.5),
        major: DVec2::new(4.0, 3.0),
        ratio: 0.4,
        start: -1.0,
        end: 3.0,
    };
    let spl = {
        let s = stroke(Curve::Nurbs(Arc::new(cubic_nurbs())), 0.0, 1.0);
        out(&s, ViewKind::Front, 0.05, 0.95)
    };
    for (o, t0, t1) in [(arc, 0.7, 4.0), (ell, -1.0, 3.0), (spl.clone(), 0.05, 0.95)] {
        let (lo, hi) = o.bounds();
        let (dlo, dhi) = dense(&o, t0, t1);
        assert!(
            close(lo, dlo, 1e-6) && close(hi, dhi, 1e-6),
            "{o:?}: {lo:?} {hi:?} / {dlo:?} {dhi:?}"
        );
        assert!(lo.x <= dlo.x + 1e-12 && hi.x >= dhi.x - 1e-12);
    }
    // Länge des Splines gegen eigenen Streckenzug.
    let mut len = 0.0;
    let mut prev = spl.point(0.05);
    for i in 1..=200_000 {
        let q = spl.point(0.05 + 0.9 * i as f64 / 200_000.0);
        len += q.distance(prev);
        prev = q;
    }
    assert!(
        (spl.length() - len).abs() < 1e-6,
        "{} {}",
        spl.length(),
        len
    );
}

// ---------------------------------------------------------------------------------------------
// Blattmodell
// ---------------------------------------------------------------------------------------------

fn line_stroke(a: DVec3, b: DVec3) -> Stroke {
    let d = b - a;
    stroke(
        Curve::Line {
            origin: a,
            dir: d.normalize(),
        },
        0.0,
        d.length(),
    )
}

/// ViewLines von Hand: Linienstücke mit gegebener Sichtbarkeit; Hüllbox aus den Endpunkten.
fn view_lines(kind: ViewKind, segs: &[(DVec3, DVec3, Visibility)]) -> ViewLines {
    let basis = Basis::of(kind);
    let mut strokes = Vec::new();
    let mut pieces = Vec::new();
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for (i, (a, b, vis)) in segs.iter().enumerate() {
        let s = line_stroke(*a, *b);
        pieces.push(Piece {
            stroke: i,
            t0: s.t0,
            t1: s.t1,
            vis: *vis,
        });
        for p in [img(*a, kind), img(*b, kind)] {
            lo = lo.min(p);
            hi = hi.max(p);
        }
        strokes.push(s);
    }
    ViewLines {
        kind,
        basis,
        extent: 100.0,
        strokes,
        pieces,
        bbox: ((lo.x, lo.y), (hi.x, hi.y)),
        centerlines: Vec::new(),
        dropped_splits: 0,
    }
}

fn drawing(projection: Projection) -> Drawing {
    Drawing::new(
        DrawingId(1),
        "Z".into(),
        Sheet {
            paper: Paper::A4,
            landscape: true,
        },
        projection,
    )
}

#[test]
fn hidden_pieces_follow_show_hidden() {
    let mut d = drawing(Projection::First);
    let id = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let vl = view_lines(
        ViewKind::Front,
        &[
            (v3(0.0, 0.0, 0.0), v3(10.0, 0.0, 0.0), Visibility::Visible),
            (v3(0.0, 0.0, 4.0), v3(10.0, 0.0, 4.0), Visibility::Hidden),
            (v3(0.0, 0.0, 8.0), v3(10.0, 0.0, 8.0), Visibility::Visible),
        ],
    );
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    // Normansicht: verdeckte Kanten sind vorgegeben an; hier ausgeschaltet.
    assert!(d.views[0].show_hidden);
    d.views[0].show_hidden = false;
    let m = build_sheet(&d, &res);
    assert_eq!(m.views.len(), 1);
    assert_eq!(m.views[0].lines.len(), 2);
    assert!(
        m.views[0]
            .lines
            .iter()
            .all(|l| l.vis == Visibility::Visible)
    );
    d.views[0].show_hidden = true;
    let m = build_sheet(&d, &res);
    let l = &m.views[0].lines;
    assert_eq!(l.len(), 3);
    assert_eq!(
        l.iter().map(|x| x.vis).collect::<Vec<_>>(),
        [Visibility::Visible, Visibility::Hidden, Visibility::Visible]
    );
}

#[test]
fn aligned_view_sits_below_front_in_first_angle() {
    let mut d = drawing(Projection::First);
    let front = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 200.0 },
        None,
    );
    let top = d.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 1 },
        Placement::Aligned { distance: 120.0 },
        Some(front),
    );
    let vf = view_lines(
        ViewKind::Front,
        &[(v3(0.0, 0.0, 0.0), v3(30.0, 0.0, 20.0), Visibility::Visible)],
    );
    let vt = view_lines(
        ViewKind::Top,
        &[(v3(0.0, 0.0, 0.0), v3(30.0, 10.0, 0.0), Visibility::Visible)],
    );
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(front, &vf), (top, &vt)]);
    let cf = view_center(&d, front, &res).unwrap();
    let ct = view_center(&d, top, &res).unwrap();
    assert!(close(cf, DVec2::new(100.0, 200.0), 1e-12));
    // Erstwinkel: Draufsicht unter der Vorderansicht, gleiche x, y um `distance` kleiner.
    assert!(close(ct, DVec2::new(100.0, 80.0), 1e-12), "{ct:?}");
    // Auf dem Blatt: die Mitte der Hüllbox der Linien liegt bei der Mitte der Ansicht.
    let m = build_sheet(&d, &res);
    assert_eq!(m.views.len(), 2);
    let mid = |v: &planora_drawing::sheet::SheetView| {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for l in &v.lines {
            let (a, b) = l.curve.bounds();
            lo = lo.min(a);
            hi = hi.max(b);
        }
        (lo + hi) * 0.5
    };
    assert!(close(mid(&m.views[0]), DVec2::new(100.0, 200.0), 1e-9));
    assert!(close(mid(&m.views[1]), DVec2::new(100.0, 80.0), 1e-9));
    // Dritter Winkel: darüber.
    d.projection = Projection::Third;
    let ct = view_center(&d, top, &res).unwrap();
    assert!(close(ct, DVec2::new(100.0, 320.0), 1e-12), "{ct:?}");
    // Elternbild nicht berechnet: die abgeleitete Ansicht entfällt.
    let only_top: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(top, &vt)]);
    assert!(view_center(&d, top, &only_top).is_none());
    let m = build_sheet(&d, &only_top);
    assert!(m.views.is_empty());
}

#[test]
fn scale_is_applied() {
    let mut d = drawing(Projection::First);
    let id = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 2 },
        Placement::Free { x: 50.0, y: 60.0 },
        None,
    );
    let vl = view_lines(
        ViewKind::Front,
        &[(v3(0.0, 0.0, 0.0), v3(10.0, 0.0, 0.0), Visibility::Visible)],
    );
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    let m = build_sheet(&d, &res);
    let l = &m.views[0].lines[0];
    let Out2d::Line { a, b } = &l.curve else {
        panic!("Line erwartet")
    };
    assert!((l.curve.length() - 5.0).abs() < 1e-12);
    // Blatt = ½·(p − (5, 0)) + (50, 60).
    assert!(close(*a, DVec2::new(47.5, 60.0), 1e-12), "{a:?}");
    assert!(close(*b, DVec2::new(52.5, 60.0), 1e-12), "{b:?}");
    assert!((m.views[0].extent_mm.0 - 5.0).abs() < 1e-12 && m.views[0].extent_mm.1.abs() < 1e-12);
    // Bogen und Ellipse skalieren Radius und Hauptachse.
    let s = stroke(
        circle(
            frame(v3(4.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)),
            6.0,
        ),
        0.0,
        TAU,
    );
    let xf = SheetTransform {
        scale: 0.5,
        tx: 50.0,
        ty: 60.0,
        bbox_center: DVec2::new(1.0, 2.0),
    };
    let v = piece_to_outs(&s, &Basis::of(ViewKind::Top), 0.0, TAU, &xf);
    let Some(Out2d::Arc { c, r, .. }) = v.first().cloned() else {
        panic!("Arc erwartet")
    };
    assert!((r - 3.0).abs() < 1e-12);
    assert!(close(
        c,
        DVec2::new(0.5 * 3.0 + 50.0, 0.5 * (0.0 - 2.0) + 60.0),
        1e-12
    ));
}

#[test]
fn names_frame_and_missing_results() {
    let mut d = drawing(Projection::First);
    let sc = Scale { num: 1, den: 1 };
    let f1 = d.add_view(
        ViewKind::Front,
        sc,
        Placement::Free { x: 50.0, y: 50.0 },
        None,
    );
    let top = d.add_view(
        ViewKind::Top,
        sc,
        Placement::Free { x: 50.0, y: 120.0 },
        None,
    );
    let f2 = d.add_view(
        ViewKind::Front,
        sc,
        Placement::Free { x: 150.0, y: 50.0 },
        None,
    );
    let f3 = d.add_view(
        ViewKind::Front,
        sc,
        Placement::Free { x: 150.0, y: 120.0 },
        None,
    );
    let seg = [(v3(0.0, 0.0, 0.0), v3(10.0, 0.0, 5.0), Visibility::Visible)];
    let vf = view_lines(ViewKind::Front, &seg);
    let vt = view_lines(ViewKind::Top, &seg);
    // f2 hat kein Ergebnis: es fehlt, behält aber seinen Namensplatz, f3 heißt „Vorne 3“.
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f1, &vf), (top, &vt), (f3, &vf)]);
    let m = build_sheet(&d, &res);
    let names: Vec<&str> = m.views.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["Vorne", "Oben", "Vorne 3"]);
    assert_eq!(m.views[2].id, f3);
    let _ = f2;
    // Der Blattrahmen kommt erst mit dem Schriftfeld dazu; ohne ihn ist `frame` leer.
    assert!(m.frame.is_none());
    assert_eq!(m.sheet, d.sheet);
}

// ---------------------------------------------------------------------------------------------
// Härtung
// ---------------------------------------------------------------------------------------------

#[test]
fn non_finite_input_does_not_panic() {
    let good = frame(DVec3::ZERO, v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0));
    let nan_frame = frame(v3(f64::NAN, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0));
    let b = Basis::of(ViewKind::Top);
    let strokes = [
        stroke(circle(good, f64::NAN), 0.0, TAU),
        stroke(circle(good, f64::INFINITY), 0.0, TAU),
        stroke(circle(nan_frame, 1.0), 0.0, TAU),
        stroke(circle(good, 1.0), 0.0, TAU),
        stroke(
            Curve::Line {
                origin: v3(f64::NAN, 0.0, 0.0),
                dir: v3(1.0, 0.0, 0.0),
            },
            0.0,
            1.0,
        ),
        stroke(Curve::Nurbs(Arc::new(cubic_nurbs())), 0.0, 1.0),
        stroke(
            Curve::Ellipse {
                frame: good,
                major: f64::NAN,
                minor: 1.0,
            },
            0.0,
            1.0,
        ),
    ];
    for s in &strokes {
        for (t0, t1) in [
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
            (0.0, 1.0),
            (1.0, 1.0),
        ] {
            let _ = piece_to_outs(s, &b, t0, t1, &xf_id());
        }
        for xf in [
            SheetTransform {
                scale: f64::NAN,
                ..xf_id()
            },
            SheetTransform {
                bbox_center: DVec2::new(f64::NAN, 0.0),
                ..xf_id()
            },
        ] {
            assert!(piece_to_outs(s, &b, 0.0, 1.0, &xf).is_empty());
        }
    }
    // Nicht endliche Eingabe → nichts zu zeichnen (keine Ersatzlinie).
    assert!(piece_to_outs(&strokes[0], &b, 0.0, TAU, &xf_id()).is_empty());
    assert_eq!(piece_to_outs(&strokes[3], &b, 0.0, TAU, &xf_id()).len(), 1);

    // Blattmodell: unbrauchbare Stücke, Maßstab 0, nicht endliche Lage und Hüllbox.
    let mut d = drawing(Projection::First);
    let ok = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 10.0, y: 10.0 },
        None,
    );
    let bad_scale = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 0 },
        Placement::Free { x: 10.0, y: 10.0 },
        None,
    );
    let bad_pos = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free {
            x: f64::NAN,
            y: 0.0,
        },
        None,
    );
    let bad_box = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 1.0, y: 1.0 },
        None,
    );
    let cyc_a = d.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 1 },
        Placement::Aligned { distance: 5.0 },
        None,
    );
    let mut vl = view_lines(
        ViewKind::Front,
        &[(v3(0.0, 0.0, 0.0), v3(10.0, 0.0, 0.0), Visibility::Visible)],
    );
    vl.strokes.push(strokes[4].clone());
    vl.pieces.push(Piece {
        stroke: 1,
        t0: 0.0,
        t1: 1.0,
        vis: Visibility::Visible,
    });
    vl.pieces.push(Piece {
        stroke: 99,
        t0: 0.0,
        t1: 1.0,
        vis: Visibility::Visible,
    });
    let mut vbox = view_lines(
        ViewKind::Front,
        &[(v3(0.0, 0.0, 0.0), v3(10.0, 0.0, 0.0), Visibility::Visible)],
    );
    vbox.bbox = ((f64::NAN, 0.0), (1.0, 1.0));
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([
        (ok, &vl),
        (bad_scale, &vl),
        (bad_pos, &vl),
        (bad_box, &vbox),
        (cyc_a, &vl),
    ]);
    let m = build_sheet(&d, &res);
    assert_eq!(m.views.len(), 1);
    assert_eq!(m.views[0].id, ok);
    assert_eq!(m.views[0].lines.len(), 1, "nur das endliche Stück bleibt");
    // Zyklus in den Elternverweisen (fremde Datei) ergibt None statt Endlosschleife.
    d.views[4].parent = Some(cyc_a);
    assert!(view_center(&d, cyc_a, &res).is_none());
}

// ---------------------------------------------------------------------------------------------
// Naht einer geschlossenen Freiformkurve (Überarbeitung 1)
// ---------------------------------------------------------------------------------------------

/// Geschlossene kubische Kurve in der XY-Ebene, Bereich [0, 1], Anfang = Ende = (10, 0, 0).
fn closed_loop() -> NurbsCurve {
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
        pts.iter().map(|&(x, y)| v3(x, y, 0.0)).collect(),
        vec![1.0; 7],
    )
    .unwrap()
}

/// Bild des Kurvenpunkts bei `t` in der Ansicht Oben, ohne die Bibliothek zu fragen: der
/// Parameter wird von Hand in [0, 1] gefaltet und die Kurve von planora-math ausgewertet.
fn loop_image(c: &NurbsCurve, t: f64) -> DVec2 {
    let u = t - t.floor();
    let u = if t >= 1.0 && u == 0.0 { 1.0 } else { u };
    img(c.point(u), ViewKind::Top)
}

fn polyline_len(c: &NurbsCurve, t0: f64, t1: f64) -> f64 {
    let n = 200_000;
    let mut len = 0.0;
    let mut prev = loop_image(c, t0);
    for i in 1..=n {
        let p = loop_image(c, t0 + (t1 - t0) * i as f64 / n as f64);
        len += p.distance(prev);
        prev = p;
    }
    len
}

/// Prüft, dass die Kurven zusammen das Stück `[t0, t1]` (gefaltet) abbilden: 100 Punkte, dazu
/// Gesamtlänge gegen einen eigenen Streckenzug.
fn check_seam(t0: f64, t1: f64, parts: &[(f64, f64)]) {
    let c = closed_loop();
    let s = stroke(Curve::Nurbs(Arc::new(c.clone())), 0.0, 1.0);
    let b = Basis::of(ViewKind::Top);
    let outs = piece_to_outs(&s, &b, t0, t1, &xf_id());
    assert_eq!(outs.len(), parts.len(), "{outs:?}");
    for (o, &(p0, p1)) in outs.iter().zip(parts) {
        let Out2d::Spline { knots, degree, .. } = o else {
            panic!("Spline erwartet")
        };
        assert!((knots[0] - p0).abs() < 1e-12);
        assert!((*knots.last().unwrap() - p1).abs() < 1e-12);
        // Randknoten mit Vielfachheit Grad + 1.
        assert!(knots[..=*degree].iter().all(|&k| k == knots[0]));
        assert!(
            knots[knots.len() - degree - 1..]
                .iter()
                .all(|&k| k == *knots.last().unwrap())
        );
    }
    let mut worst: f64 = 0.0;
    for i in 0..100 {
        let t = t0 + (t1 - t0) * i as f64 / 99.0;
        // Bezug: Punkt des Strichs (Kernel faltet selbst) gegen die eigene Faltung von Hand.
        let want = s.point2(&b, t);
        assert!(
            close(want, loop_image(&c, t), 1e-9),
            "Bezugswerte uneins bei {t}"
        );
        // Die Kurve, deren Bereich t (um ganze Perioden verschoben) enthält, muss ihn treffen.
        let got = outs
            .iter()
            .zip(parts)
            .filter_map(|(o, &(p0, p1))| {
                let u = t - (t - 0.5 * (p0 + p1)).round();
                (u >= p0 - 1e-12 && u <= p1 + 1e-12).then(|| o.point(u))
            })
            .map(|g| g.distance(want))
            .fold(f64::INFINITY, f64::min);
        worst = worst.max(got);
    }
    assert!(worst < 1e-9, "größte Abweichung {worst}");
    let total: f64 = outs.iter().map(Out2d::length).sum();
    let want = polyline_len(&c, t0, t1);
    assert!((total - want).abs() < 1e-5, "{total} {want}");
}

#[test]
fn piece_across_the_seam_of_a_closed_spline_is_split_in_two() {
    check_seam(0.8, 1.2, &[(0.8, 1.0), (0.0, 0.2)]);
}

#[test]
fn seam_pieces_in_other_positions() {
    // Vor dem Anfang (gefaltet), ganz hinter dem Ende, im Bereich, volle Periode.
    check_seam(-0.1, 0.1, &[(0.9, 1.0), (0.0, 0.1)]);
    check_seam(1.1, 1.3, &[(0.1, 0.3)]);
    check_seam(0.2, 0.5, &[(0.2, 0.5)]);
    check_seam(0.0, 1.0, &[(0.0, 1.0)]);
    // Mehr als eine Periode: kommt aus compute_view nicht vor, ergibt nichts (kein Raten).
    let s = stroke(Curve::Nurbs(Arc::new(closed_loop())), 0.0, 1.0);
    assert!(piece_to_outs(&s, &Basis::of(ViewKind::Top), 0.1, 1.3, &xf_id()).is_empty());
}

#[test]
fn seam_piece_gives_two_sheet_lines() {
    let st = stroke(Curve::Nurbs(Arc::new(closed_loop())), 0.0, 1.0);
    let mut d = drawing(Projection::First);
    let id = d.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let vl = ViewLines {
        kind: ViewKind::Top,
        basis: Basis::of(ViewKind::Top),
        extent: 20.0,
        strokes: vec![st],
        pieces: vec![Piece {
            stroke: 0,
            t0: 0.8,
            t1: 1.2,
            vis: Visibility::Visible,
        }],
        bbox: ((-10.0, -10.0), (10.0, 10.0)),
        centerlines: Vec::new(),
        dropped_splits: 0,
    };
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    let m = build_sheet(&d, &res);
    assert_eq!(m.views[0].lines.len(), 2);
}

// ---------------------------------------------------------------------------------------------
// Härtung der Ausgabekurven (Überarbeitung 1)
// ---------------------------------------------------------------------------------------------

#[test]
fn bounds_survive_huge_parameters() {
    for (start, end) in [(1e300, 1e300 + 1.0), (-1e300, 1e300), (f64::MAX, f64::MAX)] {
        let arc = Out2d::Arc {
            c: DVec2::ZERO,
            r: 1.0,
            start,
            end,
        };
        let ell = Out2d::Ellipse {
            c: DVec2::ZERO,
            major: DVec2::new(2.0, 0.0),
            ratio: 0.5,
            start,
            end,
        };
        for o in [arc, ell] {
            let _ = o.bounds();
            let _ = o.length();
        }
    }
    let arc = Out2d::Arc {
        c: DVec2::ZERO,
        r: 1.0,
        start: 1e12,
        end: 1e12 + TAU,
    };
    let (lo, hi) = arc.bounds();
    assert!(close(lo, DVec2::splat(-1.0), 1e-3) && close(hi, DVec2::splat(1.0), 1e-3));
}

#[test]
fn malformed_splines_do_not_panic() {
    let sp = |degree, knots: Vec<f64>, ctrl: Vec<DVec2>, weights: Vec<f64>| Out2d::Spline {
        degree,
        knots,
        ctrl,
        weights,
    };
    let two = vec![DVec2::ZERO, DVec2::X];
    let bad = [
        sp(3, vec![], vec![], vec![]),
        sp(2, vec![0.0, 0.0, 1.0, 1.0], two.clone(), vec![1.0, 1.0]),
        sp(1, vec![0.0, 0.0, 1.0], two.clone(), vec![1.0, 1.0]),
        sp(1, vec![0.0, 1.0, 0.5, 1.0], two.clone(), vec![1.0, 1.0]),
        sp(0, vec![0.0, 1.0], vec![DVec2::ZERO], vec![1.0]),
        sp(1, vec![0.0, 0.0, 1.0, 1.0], two.clone(), vec![1.0, 0.0]),
    ];
    for o in &bad {
        assert!(o.point(0.5).is_nan(), "{o:?}");
        assert!(o.length().is_nan());
        let (lo, hi) = o.bounds();
        assert!(lo.is_nan() && hi.is_nan());
        assert!(!o.is_finite());
    }
    // Das gültige Gegenstück (Grad 1) funktioniert.
    let ok = sp(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec2::ZERO, DVec2::new(3.0, 4.0)],
        vec![1.0, 1.0],
    );
    assert!((ok.length() - 5.0).abs() < 1e-9);
    assert!(close(ok.point(0.5), DVec2::new(1.5, 2.0), 1e-12));
}

#[test]
fn normalized_range_gives_start_in_one_turn_and_sweep() {
    let arc = |start: f64, end: f64| Out2d::Arc {
        c: DVec2::ZERO,
        r: 1.0,
        start,
        end,
    };
    let (s, w) = arc(-1.0, 2.0).normalized_range().unwrap();
    assert!((s - (TAU - 1.0)).abs() < 1e-12 && (w - 3.0).abs() < 1e-12);
    let (s, w) = arc(7.0, 7.0 + TAU).normalized_range().unwrap();
    assert!((s - (7.0 - TAU)).abs() < 1e-12 && (w - TAU).abs() < 1e-12);
    let (s, w) = arc(0.5, 1.0).normalized_range().unwrap();
    assert!((s - 0.5).abs() < 1e-15 && (w - 0.5).abs() < 1e-15);
    let e = Out2d::Ellipse {
        c: DVec2::ZERO,
        major: DVec2::X,
        ratio: 0.5,
        start: -TAU - 0.25,
        end: -TAU + 0.25,
    };
    let (s, w) = e.normalized_range().unwrap();
    assert!((s - (TAU - 0.25)).abs() < 1e-12 && (w - 0.5).abs() < 1e-12);
    assert!(arc(f64::NAN, 1.0).normalized_range().is_none());
    let line = Out2d::Line {
        a: DVec2::ZERO,
        b: DVec2::X,
    };
    assert!(line.normalized_range().is_none());
}

#[test]
fn parent_that_is_not_drawn_does_not_position_children() {
    let mut d = drawing(Projection::First);
    let sc = Scale { num: 1, den: 1 };
    let front = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 0 },
        Placement::Free { x: 100.0, y: 200.0 },
        None,
    );
    let top = d.add_view(
        ViewKind::Top,
        sc,
        Placement::Aligned { distance: 50.0 },
        Some(front),
    );
    let seg = [(v3(0.0, 0.0, 0.0), v3(10.0, 0.0, 5.0), Visibility::Visible)];
    let vf = view_lines(ViewKind::Front, &seg);
    let vt = view_lines(ViewKind::Top, &seg);
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(front, &vf), (top, &vt)]);
    assert!(view_center(&d, top, &res).is_none());
    assert!(build_sheet(&d, &res).views.is_empty());
    // Mit gültigem Maßstab erscheinen beide.
    d.views[0].scale = sc;
    assert!(view_center(&d, top, &res).is_some());
    assert_eq!(build_sheet(&d, &res).views.len(), 2);
}

#[test]
fn hidden_lines_default_on_for_normal_views_off_for_isometric() {
    // Festlegung: Zeichnungen für die Werkstatt zeigen verdeckte Kanten in den Normansichten
    // von Anfang an; die isometrische Ansicht nicht.
    let mut d = drawing(Projection::First);
    for k in ViewKind::ALL {
        let id = d.add_view(
            k,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        );
        let want = !matches!(k, ViewKind::Isometric);
        assert_eq!(d.view(id).unwrap().show_hidden, want, "{k:?}");
    }
}
