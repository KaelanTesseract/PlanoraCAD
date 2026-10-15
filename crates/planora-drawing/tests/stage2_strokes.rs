//! Stufe 2: Striche aus Körperkanten, Nähte und glatte Kanten.

mod common;
use common::model::*;
use planora_drawing::{DrawBody, basis::Basis, model::ViewKind, stroke::*};

fn bodies(s: &planora_kernel::Solid) -> Vec<DrawBody<'_>> {
    vec![DrawBody {
        name: "K".into(),
        solid: s,
    }]
}

#[test]
fn box_has_twelve_edge_strokes() {
    let s = box_solid(10.0, 20.0, 30.0);
    let b = bodies(&s);
    let e = extent_of(&b);
    let strokes = edge_strokes(&b, &Basis::of(ViewKind::Front), e).unwrap();
    assert_eq!(strokes.len(), 12);
    assert!((e - 30.0).abs() < 1e-9);
}

#[test]
fn front_view_of_box_projects_to_rectangle_lengths() {
    let s = box_solid(10.0, 20.0, 30.0);
    let b = bodies(&s);
    let basis = Basis::of(ViewKind::Front);
    let strokes = edge_strokes(&b, &basis, extent_of(&b)).unwrap();
    // Vorne: 4 Kanten 10 lang (parallel X), 4 Kanten 30 hoch (Z), 4 Kanten längs Y fallen zu Punkten
    let degenerate = strokes
        .iter()
        .filter(|s| s.is_degenerate_2d(&basis, 1e-9))
        .count();
    assert_eq!(degenerate, 4);
    let total = total_len(&strokes, &basis);
    assert!((total - (4.0 * 10.0 + 4.0 * 30.0)).abs() < 1e-6, "{total}");
}

#[test]
fn cylinder_has_circle_edges_and_no_seam_stroke() {
    let s = cylinder_solid(5.0, 12.0);
    let b = bodies(&s);
    let strokes = edge_strokes(&b, &Basis::of(ViewKind::Top), extent_of(&b)).unwrap();
    // zwei Kreiskanten; die Naht des Mantels ist keine Strichkante
    assert_eq!(strokes.len(), 2);
    assert!(
        strokes
            .iter()
            .all(|s| matches!(s.curve, planora_kernel::Curve::Circle { .. }))
    );
}

#[test]
fn top_view_circle_projects_to_circle_length() {
    let s = cylinder_solid(5.0, 12.0);
    let b = bodies(&s);
    let basis = Basis::of(ViewKind::Top);
    let strokes = edge_strokes(&b, &basis, extent_of(&b)).unwrap();
    let total = total_len(&strokes, &basis);
    assert!((total - 2.0 * 2.0 * std::f64::consts::PI * 5.0).abs() < 1e-6);
}

#[test]
fn filleted_box_does_not_draw_tangent_edges() {
    // Quader mit einer verrundeten Kante: die beiden Tangentenkanten (Rundung ↔ Seitenflächen) sind glatt und entfallen.
    let s = box_with_one_rounded_edge(20.0, 20.0, 20.0, 3.0);
    let b = bodies(&s);
    let all = edge_strokes(&b, &Basis::of(ViewKind::Front), extent_of(&b)).unwrap();
    let smooth = count_smooth_edges(&s); // eigener Prüfer: Winkel der Flächennormalen an der Kante
    assert_eq!(
        all.len(),
        s.edges.iter().count() - smooth - count_seam_edges(&s)
    );
    assert!(smooth >= 2);
}

#[test]
fn extent_scales_with_body() {
    let big = box_solid(1.0e5, 2.0e5, 3.0e5);
    let small = box_solid(0.1, 0.2, 0.3);
    assert!((extent_of(&bodies(&big)) - 3.0e5).abs() < 1e-3);
    assert!((extent_of(&bodies(&small)) - 0.3).abs() < 1e-12);
}

fn count(s: &planora_kernel::Solid, kind: ViewKind) -> usize {
    let b = bodies(s);
    edge_strokes(&b, &Basis::of(kind), extent_of(&b))
        .unwrap()
        .len()
}

#[test]
fn round_bodies_keep_only_their_real_edges() {
    // Kugel und Torus: nur Nähte, keine Strichkante.
    assert_eq!(count(&sphere_solid(7.0), ViewKind::Front), 0);
    assert_eq!(count(&torus_solid(10.0, 3.0), ViewKind::Front), 0);
    // Kegelstumpf: zwei Kreiskanten.
    assert_eq!(count(&cone_solid(6.0, 3.0, 9.0), ViewKind::Front), 2);
    // Halbzylinder: je 2 Kanten unten und oben, 2 senkrechte (Ecken 90°).
    assert_eq!(count(&half_cylinder_solid(4.0, 9.0), ViewKind::Front), 6);
    // Platte mit Freiformwand: vier Grundkanten, oben und unten, vier senkrechte.
    let (slab, face) = nurbs_slab_solid();
    assert!(matches!(
        slab.faces.iter().nth(face).map(|(_, f)| &f.surface),
        Some(planora_kernel::Surface::Nurbs(_))
    ));
    assert_eq!(count(&slab, ViewKind::Front), 12);
    let (p, n) = nurbs_slab_center_point_and_normal();
    let wall = &slab.faces.iter().nth(face).unwrap().1;
    let (u, v) = wall.surface.project(p);
    assert!((wall.surface.point(u, v) - p).length() < 1e-6);
    let nn = wall.surface.normal(u, v) * if wall.same_sense { 1.0 } else { -1.0 };
    assert!(nn.dot(n) > 1.0 - 1e-6);
}

#[test]
fn stroke_derivative_and_bounds_match_the_curve() {
    let basis = Basis::of(ViewKind::Isometric);
    let (slab, _) = nurbs_slab_solid();
    let cyl = cylinder_solid(5.0, 12.0);
    for s in [&slab, &cyl] {
        let b = bodies(s);
        let e = extent_of(&b);
        let pad = planora_drawing::limits::geom_tol(e);
        for st in edge_strokes(&b, &basis, e).unwrap() {
            let (lo, hi) = st.bbox2(&basis, pad);
            for i in 0..=10 {
                let t = st.t0 + (st.t1 - st.t0) * i as f64 / 10.0;
                let p = st.point2(&basis, t);
                assert!(p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y);
                let h = (st.t1 - st.t0).abs() * 1e-6;
                let (lo_t, hi_t) = (st.t0.min(st.t1), st.t0.max(st.t1));
                let tc = t.clamp(lo_t + 2.0 * h, hi_t - 2.0 * h);
                let (tm, tp) = (tc - h, tc + h);
                let num = (st.point2(&basis, tp) - st.point2(&basis, tm)) / (tp - tm);
                let ana = st.deriv2(&basis, (tm + tp) / 2.0);
                assert!(
                    (num - ana).length() < 1e-5 * (1.0 + ana.length()),
                    "{num} {ana} t={t}"
                );
            }
            assert!((st.param_len() - (st.t1 - st.t0).abs()).abs() < 1e-15);
        }
    }
}

#[test]
fn an_edge_with_one_face_is_an_invalid_body() {
    let mut s = box_solid(1.0, 1.0, 1.0);
    // Eine Fläche samt Halbkanten entfernen: Ersatz durch leere Schleife.
    let first = s.faces.iter().next().unwrap().0;
    s.faces[first].outer.half_edges.clear();
    let b = bodies(&s);
    let r = edge_strokes(&b, &Basis::of(ViewKind::Front), extent_of(&b));
    assert!(matches!(
        r,
        Err(planora_drawing::DrawError::InvalidBody(m)) if m == "Kante ohne zwei Flächen"
    ));
}

fn arc_stroke(curve: planora_kernel::Curve, t0: f64, t1: f64) -> Stroke {
    Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

#[test]
fn conic_bbox_is_conservative_and_tight_between_samples() {
    use planora_kernel::Curve;
    use planora_math::{DVec3, Frame};
    let frame = Frame {
        origin: DVec3::new(3.0, -2.0, 1.0),
        ..Frame::from_origin_normal(DVec3::new(3.0, -2.0, 1.0), DVec3::new(1.0, 2.0, 3.0))
    };
    let pad = 1e-9;
    for basis in [Basis::of(ViewKind::Isometric), Basis::of(ViewKind::Front)] {
        let arcs = [
            arc_stroke(Curve::Circle { frame, radius: 5.0 }, 0.37, 5.91),
            arc_stroke(Curve::Circle { frame, radius: 5.0 }, -1.3, 0.9),
            arc_stroke(
                Curve::Ellipse {
                    frame,
                    major: 7.0,
                    minor: 2.5,
                },
                0.21,
                4.77,
            ),
            arc_stroke(
                Curve::Ellipse {
                    frame,
                    major: 7.0,
                    minor: 2.5,
                },
                0.0,
                std::f64::consts::TAU,
            ),
        ];
        for st in &arcs {
            let (lo, hi) = st.bbox2(&basis, pad);
            let n = 100_000;
            let (mut tl, mut th) = (
                planora_math::DVec2::splat(f64::INFINITY),
                planora_math::DVec2::splat(f64::NEG_INFINITY),
            );
            for i in 0..=n {
                let p = st.point2(&basis, st.t0 + (st.t1 - st.t0) * i as f64 / n as f64);
                assert!(p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y);
                tl = tl.min(p);
                th = th.max(p);
            }
            // Dicht abgetastete wahre Ausdehnung (Abtastfehler < 1e-8): Box höchstens pad + 1e-12 größer.
            for (outer, inner) in [(lo.x, tl.x), (lo.y, tl.y), (th.x, hi.x), (th.y, hi.y)] {
                let slack = inner - outer;
                assert!(slack >= 0.0 && slack <= pad + 1e-12 + 1e-8, "{slack}");
            }
        }
    }
}

#[test]
fn ellipse_derivative_matches_central_difference() {
    use planora_kernel::Curve;
    use planora_math::{DVec3, Frame};
    let frame = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 2.0, 3.0));
    let st = arc_stroke(
        Curve::Ellipse {
            frame,
            major: 7.0,
            minor: 2.5,
        },
        0.2,
        4.0,
    );
    let basis = Basis::of(ViewKind::Isometric);
    for i in 0..=20 {
        let t = 0.3 + 3.5 * i as f64 / 20.0;
        let h = 1e-6;
        let num = (st.point2(&basis, t + h) - st.point2(&basis, t - h)) / (2.0 * h);
        assert!((num - st.deriv2(&basis, t)).length() < 1e-6);
    }
}
