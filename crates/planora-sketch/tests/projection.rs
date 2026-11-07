//! Skizzen 6, Task 3: beliebige Kurvenstücke projizieren (auch Ellipsenbögen) und verknüpfte,
//! profilfähige Projektionen (`construction: false`).

use planora_kernel::Curve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::solver::SolveStatus;
use std::f64::consts::FRAC_PI_2;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

/// Ebene durch den Ursprung, um die x-Achse um 45° gekippt (x bleibt die Weltachse x).
fn tilted_45() -> Frame {
    let (s, c) = (PI / 4.0).sin_cos();
    Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::new(0.0, c, s),
        z: DVec3::new(0.0, -s, c),
    }
}

/// Kreis vom Radius `r` in der gekippten Ebene.
fn tilted_circle(r: f64) -> Curve {
    Curve::Circle {
        frame: tilted_45(),
        radius: r,
    }
}

/// Punkt `curve(t)` in Skizzenkoordinaten (Skizze in der Weltebene z = 0).
fn projected(curve: &Curve, t: f64) -> DVec2 {
    curve.point(t).truncate()
}

/// Mitte, Start und Ende eines Ellipsenbogens.
fn arc_points(s: &Sketch, e: CurveId) -> (DVec2, DVec2, DVec2) {
    let CurveKind::EllipticArc {
        center, start, end, ..
    } = s.curve(e).unwrap().kind
    else {
        panic!("Ellipsenbogen erwartet: {:?}", s.curve(e).unwrap().kind)
    };
    (
        s.point_xy(center).unwrap(),
        s.point_xy(start).unwrap(),
        s.point_xy(end).unwrap(),
    )
}

fn major(s: &Sketch, e: CurveId) -> f64 {
    s.measure(&K::EllipseMajor(e, 0.0)).unwrap()
}

fn minor(s: &Sketch, e: CurveId) -> f64 {
    s.measure(&K::EllipseMinor(e, 0.0)).unwrap()
}

/// Die Skizze ist gelöst, voll bestimmt und ohne überzählige Abhängigkeiten.
fn assert_solved_rigid(s: &mut Sketch) {
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert_eq!(rep.dof, 0, "{rep:?}");
}

#[test]
fn oblique_arc_projects_to_elliptic_arc() {
    let curve = tilted_circle(10.0);
    let mut s = sketch();
    let e = s
        .add_projected_curve(&curve, 0.0, FRAC_PI_2, LINKED)
        .unwrap();
    let (m, a, b) = arc_points(&s, e);
    assert!(m.length() < 1e-9, "{m:?}");
    assert!((major(&s, e) - 10.0).abs() < 1e-9);
    assert!((minor(&s, e) - 10.0 * (PI / 4.0).cos()).abs() < 1e-9);
    assert!(a.distance(projected(&curve, 0.0)) < 1e-9, "{a:?}");
    assert!(b.distance(projected(&curve, FRAC_PI_2)) < 1e-9, "{b:?}");
    // Die Mitte des Bogens liegt auf der Projektion der Kurvenmitte (richtige Laufrichtung).
    let mid = projected(&curve, PI / 4.0);
    let t = s.ellipse_param_of(e, mid).unwrap();
    assert!(s.ellipse_point(e, t).unwrap().distance(mid) < 1e-9);
    assert!((0.0..FRAC_PI_2).contains(&t), "t = {t}");
    assert!(!s.curve(e).unwrap().construction);
    assert!(s.points().all(|(_, p)| !p.construction));
    assert_solved_rigid(&mut s);
    assert!(
        a.distance(arc_points(&s, e).1) < 1e-9,
        "Lösen bewegt nichts"
    );
}

#[test]
fn reversed_piece_keeps_the_same_arc() {
    // Läuft das Stück rückwärts (t1 < t0), ist es derselbe Bogen.
    let curve = tilted_circle(10.0);
    let mut s = sketch();
    let e = s
        .add_projected_curve(&curve, FRAC_PI_2, 0.0, LINKED)
        .unwrap();
    let (_, a, b) = arc_points(&s, e);
    assert!(a.distance(projected(&curve, 0.0)) < 1e-9, "{a:?}");
    assert!(b.distance(projected(&curve, FRAC_PI_2)) < 1e-9, "{b:?}");
}

#[test]
fn arc_turning_clockwise_in_the_sketch_swaps_its_ends() {
    // Die Ebene zeigt von der Skizze weg: Der Bogen läuft in der Skizze im Uhrzeigersinn,
    // als Ellipsenbogen (gegen den Uhrzeigersinn) also vom Ende zum Anfang.
    let (sn, cs) = (PI / 4.0).sin_cos();
    let curve = Curve::Circle {
        frame: Frame {
            origin: DVec3::ZERO,
            x: DVec3::X,
            y: DVec3::new(0.0, -cs, sn),
            z: DVec3::new(0.0, -sn, -cs),
        },
        radius: 10.0,
    };
    let mut s = sketch();
    let e = s
        .add_projected_curve(&curve, 0.0, FRAC_PI_2, LINKED)
        .unwrap();
    let (_, a, b) = arc_points(&s, e);
    assert!(a.distance(projected(&curve, FRAC_PI_2)) < 1e-9, "{a:?}");
    assert!(b.distance(projected(&curve, 0.0)) < 1e-9, "{b:?}");
    // Die Mitte liegt gegen den Uhrzeigersinn zwischen Start und Ende.
    let param = |p: DVec2| s.ellipse_param_of(e, p).unwrap();
    let (t_a, t_m, t_b) = (param(a), param(projected(&curve, PI / 4.0)), param(b));
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(2.0 * PI);
    assert!(ccw(t_a, t_m) < ccw(t_a, t_b), "{t_a} {t_m} {t_b}");
    assert_solved_rigid(&mut s);
}

#[test]
fn partial_ellipse_in_a_parallel_plane_is_an_elliptic_arc() {
    let curve = Curve::Ellipse {
        frame: Frame::from_origin_normal(DVec3::new(1.0, 2.0, 7.0), DVec3::Z),
        major: 4.0,
        minor: 2.0,
    };
    let mut s = sketch();
    let e = s
        .add_projected_curve(&curve, 0.5, 2.0, ProjectStyle { construction: true })
        .unwrap();
    let (m, a, b) = arc_points(&s, e);
    assert!(m.distance(DVec2::new(1.0, 2.0)) < 1e-9);
    assert!((major(&s, e) - 4.0).abs() < 1e-9);
    assert!((minor(&s, e) - 2.0).abs() < 1e-9);
    assert!(a.distance(projected(&curve, 0.5)) < 1e-9);
    assert!(b.distance(projected(&curve, 2.0)) < 1e-9);
    assert!(s.curve(e).unwrap().construction);
    assert!(s.points().all(|(_, p)| p.construction));
    assert_solved_rigid(&mut s);
}

#[test]
fn linked_projection_is_not_construction_and_forms_profile() {
    let mut s = sketch();
    let corners = [
        DVec3::new(0.0, 0.0, 3.0),
        DVec3::new(10.0, 0.0, 3.0),
        DVec3::new(10.0, 5.0, 3.0),
        DVec3::new(0.0, 5.0, 3.0),
    ];
    for i in 0..4 {
        let (p, q) = (corners[i], corners[(i + 1) % 4]);
        let line = Curve::Line {
            origin: p,
            dir: (q - p).normalize(),
        };
        let c = s
            .add_projected_curve(&line, 0.0, p.distance(q), LINKED)
            .unwrap();
        assert!(!s.curve(c).unwrap().construction);
    }
    assert_eq!(s.profiles().profiles.len(), 1);
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn reproject_curve_keeps_user_constraint() {
    let mut s = sketch();
    let line = |o: DVec3| Curve::Line {
        origin: o,
        dir: DVec3::X,
    };
    let c = s
        .add_projected_curve(&line(DVec3::ZERO), 0.0, 10.0, LINKED)
        .unwrap();
    let CurveKind::Line { a: end, .. } = s.curve(c).unwrap().kind else {
        panic!("Linie erwartet")
    };
    let p = s.add_point(20.0, 20.0).unwrap();
    let q = s.add_point(30.0, 25.0).unwrap();
    let free = s.add_line(p, q).unwrap();
    s.add_constraint(K::Coincident(p, end)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(s.point_xy(p).unwrap().length() < 1e-7);

    s.reproject_curve(c, &line(DVec3::new(5.0, 5.0, 2.0)), 0.0, 10.0)
        .unwrap();
    assert!(s.curve(c).is_some(), "gleiche Id");
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(s.point_xy(end).unwrap().distance(DVec2::new(5.0, 5.0)) < 1e-7);
    assert!(
        s.point_xy(p).unwrap().distance(DVec2::new(5.0, 5.0)) < 1e-7,
        "die freie Linie folgt"
    );
    assert!(s.curve(free).is_some());
    assert!((s.line_length(c).unwrap() - 10.0).abs() < 1e-7);
}

#[test]
fn elliptic_arc_reprojects() {
    let mut s = sketch();
    let e = s
        .add_projected_curve(&tilted_circle(10.0), 0.0, FRAC_PI_2, LINKED)
        .unwrap();
    let bigger = tilted_circle(20.0);
    s.reproject_curve(e, &bigger, 0.0, FRAC_PI_2).unwrap();
    assert!((major(&s, e) - 20.0).abs() < 1e-9);
    assert!((minor(&s, e) - 20.0 * (PI / 4.0).cos()).abs() < 1e-9);
    assert_solved_rigid(&mut s);
    let (_, a, b) = arc_points(&s, e);
    assert!(a.distance(projected(&bigger, 0.0)) < 1e-7, "{a:?}");
    assert!(b.distance(projected(&bigger, FRAC_PI_2)) < 1e-7, "{b:?}");
    assert!(
        (major(&s, e) - 20.0).abs() < 1e-7,
        "die Fixwerte wurden mitgeführt"
    );

    // Andere Art: Fehler, die Skizze bleibt, wie sie ist.
    let rev = s.revision();
    let line = Curve::Line {
        origin: DVec3::ZERO,
        dir: DVec3::X,
    };
    assert!(matches!(
        s.reproject_curve(e, &line, 0.0, 5.0),
        Err(SketchError::UnsupportedProjection(_))
    ));
    assert_eq!(s.revision(), rev);
    assert!((major(&s, e) - 20.0).abs() < 1e-7);
}

#[test]
fn old_edge_copy_unchanged() {
    // Die Kantenkopie bleibt Konstruktion und lehnt schräge Teilbögen weiter ab (die alten
    // Tests in `helpers.rs`, `ellipse_spline.rs` und `reproject.rs` prüfen den Rest).
    let mut solid = planora_kernel::Solid::default();
    let v0 = solid.add_vertex(DVec3::new(10.0, 0.0, 0.0));
    let v1 = solid.add_vertex(projected(&tilted_circle(10.0), FRAC_PI_2).extend(0.0));
    let edge = solid.add_edge(tilted_circle(10.0), 0.0, FRAC_PI_2, v0, v1);
    let mut s = sketch();
    assert!(matches!(
        s.add_projected_edge(&solid, edge),
        Err(SketchError::UnsupportedProjection(_))
    ));
    assert_eq!(s.points().count(), 0);
}

#[test]
fn freeform_and_degenerate_pieces_are_rejected() {
    let mut s = sketch();
    let up = Curve::Line {
        origin: DVec3::new(1.0, 1.0, 0.0),
        dir: DVec3::Z,
    };
    assert!(s.projects_to_point(&up, 0.0, 5.0));
    assert!(matches!(
        s.add_projected_curve(&up, 0.0, 5.0, LINKED),
        Err(SketchError::UnsupportedProjection(_))
    ));
    let flat = Curve::Line {
        origin: DVec3::ZERO,
        dir: DVec3::X,
    };
    assert!(!s.projects_to_point(&flat, 0.0, 5.0));
    assert!(!s.projects_to_point(&tilted_circle(1.0), 0.0, 1.0));
    assert!(matches!(
        s.add_projected_curve(&flat, 1.0, 1.0, LINKED),
        Err(SketchError::Degenerate(_))
    ));
    // Skizzen 6 (Korrektur 1): Ein Kreis, der hochkant zur Skizze steht, wird wie in Fusion
    // eine Linie (`edge_on_*` unten); abgelehnt bleiben Freiform und Punkte.
    assert_eq!(s.points().count(), 0);
}

/// Kreis vom Radius `r` um `center` in einer Ebene senkrecht zur Skizze (Normale x).
fn edge_on_circle(center: DVec3, r: f64) -> Curve {
    Curve::Circle {
        frame: Frame::from_origin_normal(center, DVec3::X),
        radius: r,
    }
}

fn line_ends(s: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (s.point_xy(a).unwrap(), s.point_xy(b).unwrap()),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

/// Die Strecke von `a` nach `b`, Richtung egal.
fn same_segment((p, q): (DVec2, DVec2), a: DVec2, b: DVec2) -> bool {
    let near = |x: DVec2, y: DVec2| x.distance(y) < 1e-9;
    (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))
}

#[test]
fn edge_on_full_circle_projects_to_a_line_of_its_diameter() {
    let mut s = sketch();
    let circle = edge_on_circle(DVec3::new(3.0, 4.0, 2.0), 2.5);
    let c = s.add_projected_curve(&circle, 0.0, TAU, LINKED).unwrap();
    assert!(matches!(s.curve(c).unwrap().kind, CurveKind::Line { .. }));
    assert!((s.line_length(c).unwrap() - 5.0).abs() < 1e-9, "Länge 2r");
    let (p, q) = line_ends(&s, c);
    // Die Kreisebene steht senkrecht auf x: die Linie läuft in y durch die Mitte.
    assert!(same_segment(
        (p, q),
        DVec2::new(3.0, 1.5),
        DVec2::new(3.0, 6.5)
    ));
    assert!(!s.curve(c).unwrap().construction);
    assert_solved_rigid(&mut s);
}

#[test]
fn edge_on_half_arc_projects_to_its_extent() {
    let mut s = sketch();
    let circle = edge_on_circle(DVec3::ZERO, 2.0);
    let Curve::Circle { frame, .. } = circle else {
        unreachable!()
    };
    // Projektion von `cf.x`, `cf.y` in die Skizze: die Richtung der Linie.
    let ax = DVec2::new(frame.x.x, frame.x.y);
    let ay = DVec2::new(frame.y.x, frame.y.y);
    let (u, a, b) = if ax.length() >= ay.length() {
        (
            ax.normalize(),
            2.0 * ax.length(),
            2.0 * ay.dot(ax.normalize()),
        )
    } else {
        (
            ay.normalize(),
            2.0 * ax.dot(ay.normalize()),
            2.0 * ay.length(),
        )
    };
    // Werte entlang der Linie für eine dichte Abtastung des Halbbogens.
    let along = |t: f64| a * t.cos() + b * t.sin();
    for (t0, t1) in [
        (0.0, PI),
        (FRAC_PI_2, FRAC_PI_2 + PI),
        (0.3, 1.2),
        (-2.0, 0.5),
    ] {
        let c = s.add_projected_curve(&circle, t0, t1, LINKED).unwrap();
        let steps = 4000;
        let vals: Vec<f64> = (0..=steps)
            .map(|k| along(t0 + (t1 - t0) * k as f64 / steps as f64))
            .collect();
        let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (p, q) = line_ends(&s, c);
        let got = (p.dot(u).min(q.dot(u)), p.dot(u).max(q.dot(u)));
        assert!(
            (got.0 - lo).abs() < 1e-5 && (got.1 - hi).abs() < 1e-5,
            "{t0}…{t1}: {got:?} ≠ ({lo}, {hi})"
        );
        // Auf der Geraden durch die Mitte.
        assert!(p.perp_dot(u).abs() < 1e-9 && q.perp_dot(u).abs() < 1e-9);
    }
}

#[test]
fn edge_on_circle_line_reprojects_when_radius_changes() {
    let mut s = sketch();
    let c = s
        .add_projected_curve(&edge_on_circle(DVec3::ZERO, 2.0), 0.0, TAU, LINKED)
        .unwrap();
    s.reproject_curve(c, &edge_on_circle(DVec3::ZERO, 3.0), 0.0, TAU)
        .unwrap();
    assert!((s.line_length(c).unwrap() - 6.0).abs() < 1e-9);
    assert_solved_rigid(&mut s);
    // Gekippt ist es eine Ellipse: Artwechsel ist ein Fehler, die Linie bleibt.
    let tilted = Curve::Circle {
        frame: Frame::from_origin_normal(DVec3::ZERO, DVec3::new(1.0, 0.0, 1.0).normalize()),
        radius: 3.0,
    };
    assert!(s.reproject_curve(c, &tilted, 0.0, TAU).is_err());
    assert!((s.line_length(c).unwrap() - 6.0).abs() < 1e-9);
}
