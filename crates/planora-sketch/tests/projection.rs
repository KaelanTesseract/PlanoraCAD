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

// ---- Skizzen 10, Task 5: 3D-Geometrie einbeziehen -------------------------------------------

use planora_sketch::helpers::{Included, include_curve};
use planora_sketch::model::PointId;

/// Der Punkt `p` trägt `Fixed3` an seiner Lage (und kein `Fixed`).
fn fixed3_at(s: &Sketch, p: PointId, at: DVec3) {
    assert!(s.is_3d_point(p), "{p:?} ist kein 3D-Punkt");
    let here = s.point_xyz(p).unwrap();
    assert!(here.distance(at) < 1e-9, "{p:?} bei {here}, nicht bei {at}");
    assert!(
        s.constraints()
            .any(|(_, d)| d.kind == K::Fixed3(p, here.x, here.y, here.z)),
        "kein Fixed3 an {p:?}"
    );
    assert!(
        !s.constraints()
            .any(|(_, d)| matches!(d.kind, K::Fixed(q, ..) if q == p))
    );
}

fn line_points(s: &Sketch, c: CurveId) -> (PointId, PointId) {
    let CurveKind::Line { a, b } = s.curve(c).unwrap().kind else {
        panic!("Linie erwartet: {:?}", s.curve(c).unwrap().kind)
    };
    (a, b)
}

#[test]
fn include_line_off_plane_gives_3d_line() {
    let curve = Curve::Line {
        origin: DVec3::new(1.0, 2.0, 3.0),
        dir: DVec3::new(1.0, 0.0, 1.0).normalize(),
    };
    let g = include_curve(&Frame::WORLD, &curve, 0.0, 5.0).unwrap();
    let (p0, p1) = (curve.point(0.0), curve.point(5.0));
    assert_eq!(g, Included::Line(p0, p1));
    let mut s = sketch();
    let ids = s.add_included(&g).unwrap();
    assert_eq!(ids.curves.len(), 1);
    let (a, b) = line_points(&s, ids.curves[0]);
    fixed3_at(&s, a, p0);
    fixed3_at(&s, b, p1);
    assert!(s.is_3d_curve(ids.curves[0]));
    assert_solved_rigid(&mut s);
    // Senkrecht zur Skizzenebene ist im Raum eine echte Linie (die Projektion lehnt sie ab).
    let up = Curve::Line {
        origin: DVec3::new(2.0, 0.0, 0.0),
        dir: DVec3::Z,
    };
    let c = s
        .add_included(&include_curve(&Frame::WORLD, &up, 0.0, 4.0).unwrap())
        .unwrap()
        .curves[0];
    let (a, b) = line_points(&s, c);
    fixed3_at(&s, a, DVec3::new(2.0, 0.0, 0.0));
    fixed3_at(&s, b, DVec3::new(2.0, 0.0, 4.0));
    // In einer gekippten Skizze in Skizzenkoordinaten.
    let frame = tilted_45();
    let mut t = Sketch::new(frame, "S", Tolerance::default());
    let g = include_curve(&frame, &curve, 0.0, 5.0).unwrap();
    assert_eq!(g, Included::Line(frame.to_local(p0), frame.to_local(p1)));
    let c = t.add_included(&g).unwrap().curves[0];
    let (a, _) = line_points(&t, c);
    fixed3_at(&t, a, frame.to_local(p0));
}

#[test]
fn include_circle_in_yz_gets_normal_x() {
    // Kreis in der yz-Ebene durch (0, 1, 2): x der Kreisebene ist y, y ist z, Normale x.
    let cf = Frame {
        origin: DVec3::new(0.0, 1.0, 2.0),
        x: DVec3::Y,
        y: DVec3::Z,
        z: DVec3::X,
    };
    let curve = Curve::Circle {
        frame: cf,
        radius: 3.0,
    };
    let g = include_curve(&Frame::WORLD, &curve, 0.0, TAU).unwrap();
    assert_eq!(
        g,
        Included::Circle {
            c: cf.origin,
            r: 3.0,
            n: DVec3::X
        }
    );
    let mut s = sketch();
    let c = s.add_included(&g).unwrap().curves[0];
    let CurveKind::Circle { center, .. } = s.curve(c).unwrap().kind else {
        panic!("Kreis erwartet")
    };
    assert_eq!(s.curve_normal(c), DVec3::X);
    fixed3_at(&s, center, cf.origin);
    assert!(s.constraints().any(|(_, d)| d.kind == K::Radius(c, 3.0)));
    assert_solved_rigid(&mut s);
    // Viertelbogen, vorwärts und rückwärts: derselbe Bogen gegen den Uhrzeigersinn um x.
    for (t0, t1) in [(0.0, FRAC_PI_2), (FRAC_PI_2, 0.0)] {
        let g = include_curve(&Frame::WORLD, &curve, t0, t1).unwrap();
        let Included::Arc { c, start, end, n } = g else {
            panic!("Bogen erwartet: {g:?}")
        };
        assert_eq!(c, cf.origin);
        assert_eq!(n, DVec3::X);
        assert!(start.distance(curve.point(0.0)) < 1e-12);
        assert!(end.distance(curve.point(FRAC_PI_2)) < 1e-12);
        let mut s = sketch();
        let arc = s.add_included(&g).unwrap().curves[0];
        let CurveKind::Arc {
            center,
            start: a,
            end: b,
        } = s.curve(arc).unwrap().kind
        else {
            panic!("Bogen erwartet")
        };
        assert_eq!(s.curve_normal(arc), DVec3::X);
        fixed3_at(&s, center, cf.origin);
        fixed3_at(&s, a, curve.point(0.0));
        fixed3_at(&s, b, curve.point(FRAC_PI_2));
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        assert_eq!(rep.dof, 0, "{rep:?}");
    }
    // Eine Normale gegen die Skizzennormale wird umgedreht, der Bogen bleibt derselbe.
    let down = Curve::Circle {
        frame: Frame {
            origin: DVec3::new(0.0, 0.0, 5.0),
            x: DVec3::X,
            y: -DVec3::Y,
            z: -DVec3::Z,
        },
        radius: 2.0,
    };
    let g = include_curve(&Frame::WORLD, &down, 0.0, FRAC_PI_2).unwrap();
    let Included::Arc { start, end, n, .. } = g else {
        panic!("Bogen erwartet: {g:?}")
    };
    assert_eq!(n, DVec3::Z);
    assert!(start.distance(down.point(FRAC_PI_2)) < 1e-12);
    assert!(end.distance(down.point(0.0)) < 1e-12);
    // Ellipse und Ellipsenbogen in der yz-Ebene.
    let ell = Curve::Ellipse {
        frame: cf,
        major: 4.0,
        minor: 2.0,
    };
    let g = include_curve(&Frame::WORLD, &ell, 0.0, TAU).unwrap();
    assert_eq!(
        g,
        Included::Ellipse {
            c: cf.origin,
            major: DVec3::new(0.0, 4.0, 0.0),
            minor: 2.0,
            n: DVec3::X
        }
    );
    let mut s = sketch();
    let e = s.add_included(&g).unwrap().curves[0];
    assert_eq!(s.curve_normal(e), DVec3::X);
    assert!(s.curve_point3(e, 0.0).unwrap().distance(ell.point(0.0)) < 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let g = include_curve(&Frame::WORLD, &ell, 0.5, 2.0).unwrap();
    assert!(matches!(g, Included::EllipticArc { n, .. } if n == DVec3::X));
    let mut s = sketch();
    let e = s.add_included(&g).unwrap().curves[0];
    let CurveKind::EllipticArc { start, end, .. } = s.curve(e).unwrap().kind else {
        panic!("Ellipsenbogen erwartet")
    };
    assert_eq!(s.curve_normal(e), DVec3::X);
    fixed3_at(&s, start, ell.point(0.5));
    fixed3_at(&s, end, ell.point(2.0));
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

/// Projektion und Einbeziehen ergeben für Geometrie in der Skizzenebene dieselbe Skizze.
#[test]
fn include_in_plane_is_plain_2d() {
    // Skizzenebenen, in denen die Rechnung genau aufgeht: Weltebene, verschoben, die xz-Ebene.
    let frames = [
        Frame::WORLD,
        Frame {
            origin: DVec3::new(3.0, -2.0, 1.5),
            ..Frame::WORLD
        },
        Frame {
            origin: DVec3::new(0.0, 4.0, 0.0),
            x: DVec3::X,
            y: DVec3::Z,
            z: -DVec3::Y,
        },
    ];
    for frame in frames {
        let at = |x: f64, y: f64| frame.point(x, y, 0.0);
        let plane = |o: DVec3, x: DVec3, y: DVec3| Frame {
            origin: o,
            x,
            y,
            z: x.cross(y),
        };
        let (fx, fy) = (frame.x, frame.y);
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let nurbs = |degree: usize, knots: Vec<f64>, pts: Vec<DVec3>, weights: Vec<f64>| {
            Curve::Nurbs(std::sync::Arc::new(
                planora_math::nurbs::NurbsCurve::new(degree, knots, pts, weights).unwrap(),
            ))
        };
        let spline = nurbs(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0, 2.0],
            vec![
                at(0.0, 0.0),
                at(2.0, 4.0),
                at(5.0, -1.0),
                at(7.0, 3.0),
                at(9.0, 0.0),
            ],
            vec![1.0; 5],
        );
        let ring = nurbs(
            2,
            vec![
                0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
            ],
            [
                (1.0, 0.0),
                (1.0, 1.0),
                (0.0, 1.0),
                (-1.0, 1.0),
                (-1.0, 0.0),
                (-1.0, -1.0),
                (0.0, -1.0),
                (1.0, -1.0),
                (1.0, 0.0),
            ]
            .iter()
            .map(|&(x, y)| at(2.0 * x, 2.0 * y))
            .collect(),
            vec![1.0, w, 1.0, w, 1.0, w, 1.0, w, 1.0],
        );
        let straight = nurbs(
            1,
            vec![0.0, 0.0, 1.0, 2.0, 2.0],
            vec![at(0.0, 0.0), at(1.0, 1.0), at(3.0, 3.0)],
            vec![1.0; 3],
        );
        let round = |o: DVec3, y: DVec3, r: f64| Curve::Circle {
            frame: plane(o, fx, y),
            radius: r,
        };
        let oval = |x: DVec3, y: DVec3| Curve::Ellipse {
            frame: plane(at(2.0, 0.0), x, y),
            major: 3.0,
            minor: 1.0,
        };
        let cases: Vec<(Curve, f64, f64)> = vec![
            (
                Curve::Line {
                    origin: at(1.0, 2.0),
                    dir: (fx * 3.0 + fy * 4.0) / 5.0,
                },
                0.0,
                5.0,
            ),
            (round(at(1.0, 1.0), fy, 2.0), 0.0, TAU),
            (round(at(1.0, 1.0), fy, 2.0), 0.3, 2.0),
            (round(at(1.0, 1.0), fy, 2.0), 2.0, 0.3),
            (round(at(-1.0, 2.0), -fy, 1.5), 0.3, 2.0),
            (oval(fy, -fx), 0.0, TAU),
            (oval(fx, fy), 0.5, 2.5),
            (oval(fx, -fy), 2.5, 0.5),
            (spline.clone(), 0.0, 2.0),
            (spline, 1.5, 0.5),
            (ring, 0.0, 1.0),
            (straight, 0.0, 2.0),
        ];
        for (curve, t0, t1) in cases {
            let mut projected = Sketch::new(frame, "S", Tolerance::default());
            projected
                .add_projected_curve(&curve, t0, t1, LINKED)
                .unwrap();
            let mut included = Sketch::new(frame, "S", Tolerance::default());
            let g = include_curve(&frame, &curve, t0, t1).unwrap();
            let ids = included.add_included(&g).unwrap();
            assert_eq!(ids.curves.len(), 1);
            assert!(!included.is_3d_curve(ids.curves[0]), "{curve:?}");
            assert_eq!(
                included.to_data(),
                projected.to_data(),
                "{curve:?} {t0}…{t1} in {frame:?}"
            );
        }
    }
}

#[test]
fn update_included_moves_points() {
    let line = |o: DVec3, d: DVec3| Curve::Line {
        origin: o,
        dir: d.normalize(),
    };
    let include = |c: &Curve, t0: f64, t1: f64| include_curve(&Frame::WORLD, c, t0, t1).unwrap();
    let mut s = sketch();
    let first = line(DVec3::new(0.0, 0.0, 1.0), DVec3::new(1.0, 1.0, 0.0));
    let c = s.add_included(&include(&first, 0.0, 3.0)).unwrap().curves[0];
    let (a, b) = line_points(&s, c);
    let next = line(DVec3::new(1.0, -1.0, 4.0), DVec3::new(0.0, 1.0, 1.0));
    s.update_included(c, &include(&next, 0.0, 2.0)).unwrap();
    assert_eq!(line_points(&s, c), (a, b));
    fixed3_at(&s, a, next.point(0.0));
    fixed3_at(&s, b, next.point(2.0));
    assert_solved_rigid(&mut s);
    // Eine 2D-Linie, deren Quelle die Ebene verlässt, wird 3D.
    let mut s = sketch();
    let flat = line(DVec3::ZERO, DVec3::X);
    let c = s.add_included(&include(&flat, 0.0, 2.0)).unwrap().curves[0];
    assert!(!s.is_3d_curve(c));
    let (a, b) = line_points(&s, c);
    s.update_included(c, &include(&first, 0.0, 3.0)).unwrap();
    fixed3_at(&s, a, first.point(0.0));
    fixed3_at(&s, b, first.point(3.0));
    assert_solved_rigid(&mut s);
    // In der Ebene geblieben: wie das Nachführen einer Projektion.
    let mut s = sketch();
    let c = s.add_included(&include(&flat, 0.0, 2.0)).unwrap().curves[0];
    let mut p = sketch();
    let pc = p.add_projected_curve(&flat, 0.0, 2.0, LINKED).unwrap();
    let moved = line(DVec3::new(1.0, 1.0, 0.0), DVec3::Y);
    s.update_included(c, &include(&moved, 0.0, 4.0)).unwrap();
    p.reproject_curve(pc, &moved, 0.0, 4.0).unwrap();
    assert_eq!(s.to_data(), p.to_data());
    // Kreis bekommt neue Mitte, Radius und Normale.
    let mut s = sketch();
    let circ = |o: DVec3, n: DVec3, r: f64| Curve::Circle {
        frame: Frame::from_origin_normal(o, n),
        radius: r,
    };
    let c = s
        .add_included(&include(&circ(DVec3::Z, DVec3::X, 2.0), 0.0, TAU))
        .unwrap()
        .curves[0];
    let CurveKind::Circle { center, .. } = s.curve(c).unwrap().kind else {
        panic!("Kreis erwartet")
    };
    let to = DVec3::new(1.0, 2.0, 3.0);
    s.update_included(c, &include(&circ(to, DVec3::Y, 5.0), 0.0, TAU))
        .unwrap();
    fixed3_at(&s, center, to);
    assert!((s.curve_normal(c) - DVec3::Y).length() < 1e-12);
    assert!(s.constraints().any(|(_, d)| d.kind == K::Radius(c, 5.0)));
    assert!(!s.constraints().any(|(_, d)| d.kind == K::Radius(c, 2.0)));
    assert_solved_rigid(&mut s);
    // Freiformkurve im Raum: neue Kurve, dieselben Endpunkte.
    let mut s = sketch();
    let bend = |z: f64| {
        Curve::Nurbs(std::sync::Arc::new(
            planora_math::nurbs::NurbsCurve::new(
                2,
                vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                vec![
                    DVec3::new(0.0, 0.0, z),
                    DVec3::new(2.0, 3.0, 2.0 * z),
                    DVec3::new(4.0, 0.0, z),
                ],
                vec![1.0; 3],
            )
            .unwrap(),
        ))
    };
    let f = s
        .add_included(&include(&bend(1.0), 0.0, 1.0))
        .unwrap()
        .curves[0];
    let CurveKind::FixedSpline { start, end, .. } = s.curve(f).unwrap().kind.clone() else {
        panic!("fester Spline erwartet")
    };
    s.update_included(f, &include(&bend(3.0), 0.0, 1.0))
        .unwrap();
    fixed3_at(&s, start, DVec3::new(0.0, 0.0, 3.0));
    fixed3_at(&s, end, DVec3::new(4.0, 0.0, 3.0));
    assert!((s.curve_point3(f, 0.5).unwrap().z - 4.5).abs() < 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    // Artwechsel: Fehler, die Skizze bleibt unverändert.
    let before = s.to_data();
    let err = s.update_included(f, &include(&first, 0.0, 3.0));
    assert!(
        matches!(err, Err(SketchError::UnsupportedProjection(_))),
        "{err:?}"
    );
    assert_eq!(s.to_data(), before);
}

/// Eingezogene Ellipsen und Ellipsenbögen im Raum sind ganz bestimmt (Entscheidung Controller).
#[test]
fn include_ellipse3_and_elliptic_arc3_have_no_dof() {
    // Senkrechte Ebene (Draufsichtwinkel wäre hier unbrauchbar) und eine schräge Ebene.
    let frames = [
        Frame {
            origin: DVec3::new(0.0, 1.0, 2.0),
            x: DVec3::Z,
            y: -DVec3::Y,
            z: DVec3::X,
        },
        Frame::from_origin_normal(
            DVec3::new(1.0, -2.0, 3.0),
            DVec3::new(1.0, 2.0, 2.0).normalize(),
        ),
    ];
    for cf in frames {
        let ell = Curve::Ellipse {
            frame: cf,
            major: 4.0,
            minor: 2.0,
        };
        // Ganz, ein allgemeiner Bogen und ein Halbbogen von Scheitel zu Scheitel.
        for (t0, t1) in [(0.0, TAU), (0.5, 2.0), (0.0, PI)] {
            let mut s = sketch();
            let g = include_curve(&Frame::WORLD, &ell, t0, t1).unwrap();
            let e = s.add_included(&g).unwrap().curves[0];
            assert!(s.is_3d_curve(e));
            assert!(
                s.internal_constraints()
                    .any(|(_, d)| matches!(d.kind, K::EllipseAxes3(c, ..) if c == e)),
                "keine interne EllipseAxes3"
            );
            assert_solved_rigid(&mut s);
            // Ziehen an einem Punkt der Kurve bewegt nichts.
            let before: Vec<DVec3> = (0..8)
                .map(|k| s.curve_point3(e, t0 + (t1 - t0) * k as f64 / 8.0).unwrap())
                .collect();
            let (CurveKind::Ellipse { center, .. } | CurveKind::EllipticArc { center, .. }) =
                s.curve(e).unwrap().kind
            else {
                panic!("Ellipse erwartet")
            };
            let at = s.point_xyz(center).unwrap();
            let _ = s.drag(&[planora_sketch::solver::Drag {
                point: center,
                target: at + DVec3::new(1.0, 1.0, 1.0),
                plane: None,
            }]);
            for (k, p) in before.iter().enumerate() {
                let q = s.curve_point3(e, t0 + (t1 - t0) * k as f64 / 8.0).unwrap();
                assert!(p.distance(q) < 1e-9, "{t0}…{t1}: {p} → {q}");
            }
            // Die Benutzer-Maße bleiben an 3D-Ellipsen gesperrt, die interne Art ist nicht wählbar.
            assert!(s.add_constraint(K::EllipseMajor(e, 4.0)).is_err());
            assert!(
                s.add_constraint(K::EllipseAxes3(e, 0.0, 0.0, 4.0, 2.0))
                    .is_err()
            );
        }
    }
    // Nachführen: Die festen Halbachsen folgen der neuen Lage, weiter ohne Freiheitsgrad.
    let mut s = sketch();
    let first = Curve::Ellipse {
        frame: frames[0],
        major: 4.0,
        minor: 2.0,
    };
    let e = s
        .add_included(&include_curve(&Frame::WORLD, &first, 0.5, 2.0).unwrap())
        .unwrap()
        .curves[0];
    let next = Curve::Ellipse {
        frame: frames[1],
        major: 5.0,
        minor: 1.0,
    };
    s.update_included(e, &include_curve(&Frame::WORLD, &next, 0.5, 2.0).unwrap())
        .unwrap();
    assert_solved_rigid(&mut s);
    assert!(
        s.curve_point3(e, s.curve_domain3(e).unwrap().0)
            .unwrap()
            .distance(next.point(0.5))
            < 1e-9
    );
    let axes: Vec<_> = s
        .internal_constraints()
        .filter(|(_, d)| matches!(d.kind, K::EllipseAxes3(c, ..) if c == e))
        .collect();
    assert_eq!(axes.len(), 1);
}

#[test]
fn update_included_point_moves_fixed3() {
    let mut s = sketch();
    // Hochliegender Punkt: 3D mit Fixed3.
    let ids = s
        .add_included(&Included::Point(DVec3::new(1.0, 2.0, 3.0)))
        .unwrap();
    assert!(ids.curves.is_empty());
    assert_eq!(ids.points.len(), 1);
    let p = ids.points[0];
    fixed3_at(&s, p, DVec3::new(1.0, 2.0, 3.0));
    s.update_included_point(p, DVec3::new(-1.0, 0.5, 7.0))
        .unwrap();
    fixed3_at(&s, p, DVec3::new(-1.0, 0.5, 7.0));
    assert_solved_rigid(&mut s);
    // In die Skizzenebene: bleibt 3D (z = 0).
    s.update_included_point(p, DVec3::new(2.0, 2.0, 0.0))
        .unwrap();
    fixed3_at(&s, p, DVec3::new(2.0, 2.0, 0.0));
    // Punkt in der Skizzenebene: 2D mit Fixed, wird beim Verlassen der Ebene 3D.
    let q = s
        .add_included(&Included::Point(DVec3::new(4.0, 5.0, 0.0)))
        .unwrap()
        .points[0];
    assert!(!s.is_3d_point(q));
    assert!(
        s.constraints()
            .any(|(_, d)| d.kind == K::Fixed(q, 4.0, 5.0))
    );
    s.update_included_point(q, DVec3::new(6.0, 5.0, 0.0))
        .unwrap();
    assert!(!s.is_3d_point(q));
    assert!(
        s.constraints()
            .any(|(_, d)| d.kind == K::Fixed(q, 6.0, 5.0))
    );
    s.update_included_point(q, DVec3::new(6.0, 5.0, -2.0))
        .unwrap();
    fixed3_at(&s, q, DVec3::new(6.0, 5.0, -2.0));
    assert_solved_rigid(&mut s);
    // Fehler lassen die Skizze unverändert.
    let before = s.to_data();
    assert!(
        s.update_included_point(q, DVec3::new(f64::NAN, 0.0, 1.0))
            .is_err()
    );
    assert_eq!(s.to_data(), before);
}
