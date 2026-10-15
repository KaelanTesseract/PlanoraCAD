//! Stufe 4: Umriss-Striche von Zylinder, Kegel und Kugel, beschnitten auf die Fläche.

mod common;
use common::model::*;
use planora_drawing::{DrawBody, basis::Basis, model::ViewKind, silhouette::*, stroke::*};
use planora_exchange::Progress;
use planora_kernel::Surface;

fn lines_of(s: &planora_kernel::Solid, kind: ViewKind) -> Vec<Stroke> {
    let b = vec![DrawBody {
        name: "K".into(),
        solid: s,
    }];
    silhouette_strokes(&b, &Basis::of(kind), extent_of(&b), &Progress::new()).unwrap()
}

/// Index der ersten Fläche mit Zylindermantel.
fn cylinder_face(s: &planora_kernel::Solid) -> usize {
    s.faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id.index())
        .expect("Zylinderfläche")
}

#[test]
fn upright_cylinder_front_has_two_silhouette_lines() {
    let s = cylinder_solid(5.0, 12.0);
    let st = lines_of(&s, ViewKind::Front);
    assert_eq!(st.len(), 2);
    let basis = Basis::of(ViewKind::Front);
    // Mantellinien bei x = ±5, z von 0 bis 12
    let mut xs: Vec<f64> = st
        .iter()
        .map(|s| s.point2(&basis, (s.t0 + s.t1) / 2.0).x)
        .collect();
    xs.sort_by(f64::total_cmp);
    assert!((xs[0] + 5.0).abs() < 1e-9 && (xs[1] - 5.0).abs() < 1e-9);
    let face = cylinder_face(&s);
    for l in &st {
        assert!(((l.point(l.t1) - l.point(l.t0)).length() - 12.0).abs() < 1e-9);
        // Enden genau auf den Randkreisen (z = 0 und z = 12).
        let (za, zb) = (l.point(l.t0).z, l.point(l.t1).z);
        assert!(za.min(zb).abs() < 1e-12 && (za.max(zb) - 12.0).abs() < 1e-12);
        assert_eq!(l.origin, StrokeOrigin::Silhouette { body: 0, face });
    }
}

#[test]
fn upright_cylinder_top_has_no_silhouette() {
    assert!(lines_of(&cylinder_solid(5.0, 12.0), ViewKind::Top).is_empty());
}

#[test]
fn tilted_view_of_cylinder_has_two_lines_at_formula_positions() {
    // Isometrisch: Mantellinien im Abstand r senkrecht zur Projektion der Achse
    let s = cylinder_solid(5.0, 12.0);
    let basis = Basis::of(ViewKind::Isometric);
    let st = lines_of(&s, ViewKind::Isometric);
    assert_eq!(st.len(), 2);
    let axis2 = planora_math::DVec2::new(basis.ex.z, basis.ey.z).normalize(); // projizierte Z-Achse
    let normal = planora_math::DVec2::new(-axis2.y, axis2.x);
    let mut offs = Vec::new();
    for l in &st {
        let a = l.point2(&basis, l.t0);
        let b = l.point2(&basis, l.t1);
        assert!((a - b).normalize().perp_dot(axis2).abs() < 1e-9); // parallel zur Achse
        let off = a.dot(normal); // Abstand zur Achse durch den Ursprung
        assert!((off.abs() - 5.0).abs() < 1e-9);
        offs.push(off);
    }
    // eine Linie auf jeder Seite der Achse
    assert!(offs[0] * offs[1] < 0.0);
}

#[test]
fn sphere_has_one_great_circle_in_every_view() {
    let s = sphere_solid(7.0);
    for k in planora_drawing::model::ViewKind::ALL {
        let st = lines_of(&s, k);
        let total: f64 = st.iter().map(|x| (x.t1 - x.t0) * 7.0).sum();
        assert!(
            (total - 2.0 * std::f64::consts::PI * 7.0).abs() < 1e-6,
            "{k:?}: {total}"
        );
        // Großkreis: jeder Punkt im Abstand 7 vom Mittelpunkt und senkrecht zur Blickrichtung.
        let d = Basis::of(k).d;
        for x in &st {
            for i in 0..=8 {
                let p = x.point(x.t0 + (x.t1 - x.t0) * i as f64 / 8.0);
                assert!((p.length() - 7.0).abs() < 1e-9 && p.dot(d).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn lower_hemisphere_keeps_the_lower_half_circle_as_one_arc() {
    // Großkreis senkrecht zu d, beschnitten auf z ≤ 0: ein Halbkreisbogen (Länge π·r) mit Enden
    // auf dem Randkreis z = 0; von oben/unten ist der Großkreis der Randkreis selbst (Rand zählt
    // als innen), also der volle Kreis.
    let r = 7.0;
    let s = lower_hemisphere_solid(r);
    for k in ViewKind::ALL {
        let st = lines_of(&s, k);
        let total: f64 = st.iter().map(|x| (x.t1 - x.t0) * r).sum();
        if matches!(k, ViewKind::Top | ViewKind::Bottom) {
            assert!(
                (total - 2.0 * std::f64::consts::PI * r).abs() < 1e-6,
                "{k:?}: {total}"
            );
            continue;
        }
        assert_eq!(
            st.len(),
            1,
            "{k:?}: {:?}",
            st.iter().map(|x| (x.t0, x.t1)).collect::<Vec<_>>()
        );
        assert!(
            (total - std::f64::consts::PI * r).abs() < 1e-6,
            "{k:?}: {total}"
        );
        let a = &st[0];
        for t in [a.t0, a.t1] {
            let p = a.point(t);
            assert!(p.z.abs() < 1e-8, "{k:?}: Ende bei z = {}", p.z);
            assert!((p.length() - r).abs() < 1e-9);
        }
        // Bogenmitte = tiefster Punkt des Großkreises: z = −r·√(1 − d_z²).
        let dz = Basis::of(k).d.z;
        let mid_z = a.point(0.5 * (a.t0 + a.t1)).z;
        assert!(
            (mid_z + r * (1.0 - dz * dz).sqrt()).abs() < 1e-6,
            "{k:?}: {mid_z}"
        );
    }
}

#[test]
fn cone_has_two_lines_from_the_side_and_none_from_the_axis() {
    let s = cone_solid(6.0, 2.0, 10.0); // Radius unten 6, oben 2, Höhe 10 (Kegelstumpf)
    let front = lines_of(&s, ViewKind::Front);
    assert_eq!(front.len(), 2);
    assert!(lines_of(&s, ViewKind::Top).is_empty());
    // Von vorne: Mantellinien von (±6, 0, 0) nach (±2, 0, 10).
    let mut ends: Vec<(f64, f64)> = front
        .iter()
        .map(|l| {
            let (a, b) = (l.point(l.t0), l.point(l.t1));
            let (lo, hi) = if a.z < b.z { (a, b) } else { (b, a) };
            assert!(lo.z.abs() < 1e-12 && (hi.z - 10.0).abs() < 1e-12);
            assert!(lo.y.abs() < 1e-9 && hi.y.abs() < 1e-9);
            (lo.x, hi.x)
        })
        .collect();
    ends.sort_by(|p, q| p.0.total_cmp(&q.0));
    assert!((ends[0].0 + 6.0).abs() < 1e-9 && (ends[0].1 + 2.0).abs() < 1e-9);
    assert!((ends[1].0 - 6.0).abs() < 1e-9 && (ends[1].1 - 2.0).abs() < 1e-9);
}

#[test]
fn half_cylinder_silhouette_is_trimmed_to_the_face() {
    // Halbzylinder: Umriss nur dort, wo `u` im Winkelbereich der Fläche liegt.
    let s = half_cylinder_solid(5.0, 12.0);
    assert_eq!(lines_of(&s, ViewKind::Front).len(), 2);
    let left = lines_of(&s, ViewKind::Left);
    assert_eq!(left.len(), 1);
    // die Linie bei u = π/2, also bei (0, 5, z)
    let m = left[0].point((left[0].t0 + left[0].t1) / 2.0);
    assert!(m.x.abs() < 1e-9 && (m.y - 5.0).abs() < 1e-9);
}

#[test]
fn torus_and_freeform_have_silhouettes() {
    // Seit Aufgabe 7 (vorher: Meldung „nicht unterstützt“). Torus von vorne (Blick in der
    // Ringebene): vier exakte Kreise – Rohrober- und -unterkante (Radius 10, Mitte (0, 0, ±3))
    // und die Meridiankreise bei x = ±10 (Radius 3).
    let s = torus_solid(10.0, 3.0);
    let st = lines_of(&s, ViewKind::Front);
    assert_eq!(st.len(), 4);
    let mut found: Vec<(f64, planora_math::DVec3)> = st
        .iter()
        .map(|x| match &x.curve {
            planora_kernel::Curve::Circle { frame, radius } => {
                assert!((x.t1 - x.t0 - std::f64::consts::TAU).abs() < 1e-12);
                (*radius, frame.origin)
            }
            other => panic!("Kreis erwartet: {other:?}"),
        })
        .collect();
    found.sort_by(|a, b| {
        (a.0, a.1.x, a.1.z)
            .partial_cmp(&(b.0, b.1.x, b.1.z))
            .unwrap()
    });
    let want = [
        (3.0, (-10.0, 0.0)),
        (3.0, (10.0, 0.0)),
        (10.0, (0.0, -3.0)),
        (10.0, (0.0, 3.0)),
    ];
    for ((r, c), (wr, (wx, wz))) in found.iter().zip(want) {
        assert!((r - wr).abs() < 1e-12 && (c.x - wx).abs() < 1e-9 && (c.z - wz).abs() < 1e-9);
        assert!(c.y.abs() < 1e-9);
    }
    // Freiformwand der Platte von vorne: senkrechte Linie bei (12, 5, z), z von 0 bis 5 (aus der
    // Symmetrie des Splines), als angepasster Spline; von oben wird die Wand nur von der Seite
    // gesehen (n·d ≡ 0) und hat keinen Umriss.
    let (slab, face) = nurbs_slab_solid();
    let st = lines_of(&slab, ViewKind::Front);
    assert_eq!(st.len(), 1);
    let l = &st[0];
    assert_eq!(l.origin, StrokeOrigin::Silhouette { body: 0, face });
    assert!(matches!(l.curve, planora_kernel::Curve::Nurbs(_)));
    for i in 0..=50 {
        let p = l.point(l.t0 + (l.t1 - l.t0) * i as f64 / 50.0);
        assert!(
            (p.x - 12.0).abs() <= 1e-4 && (p.y - 5.0).abs() <= 1e-4,
            "{p:?}"
        );
    }
    let (za, zb) = (l.point(l.t0).z, l.point(l.t1).z);
    assert!(za.min(zb).abs() < 1e-9 && (za.max(zb) - 5.0).abs() < 1e-9);
    assert!(lines_of(&slab, ViewKind::Top).is_empty());
}

#[test]
fn cancelled_progress_stops_the_silhouette() {
    let s = cylinder_solid(5.0, 12.0);
    let b = vec![DrawBody {
        name: "K".into(),
        solid: &s,
    }];
    let p = Progress::new();
    p.cancel();
    assert!(matches!(
        silhouette_strokes(&b, &Basis::of(ViewKind::Front), 12.0, &p),
        Err(planora_drawing::DrawError::Cancelled)
    ));
}

#[test]
fn slanted_cut_silhouette_ends_lie_on_the_cut_edge() {
    // Zylinder r = 5, h = 12, oben schräg abgeschnitten (Ellipsenkante, im (u, v)-Raum gekrümmt).
    // Die oberen Enden der Umrisslinien müssen genau auf der Schnittebene und auf einem Punkt
    // der Ellipsenkante liegen (sonst teilt isect die Kante dort nicht), die unteren auf z = 0.
    let s = slanted_cylinder_solid(5.0, 12.0);
    let b = vec![DrawBody {
        name: "K".into(),
        solid: &s,
    }];
    let e = extent_of(&b);
    let near = 16.0 * planora_drawing::limits::geom_tol(e);
    let (n, c) = slanted_cut_plane();
    let plane = |p: planora_math::DVec3| (n.dot(p) - c) / n.length();
    for k in [ViewKind::Front, ViewKind::Isometric] {
        let basis = Basis::of(k);
        let st = silhouette_strokes(&b, &basis, e, &Progress::new()).unwrap();
        assert_eq!(st.len(), 2, "{k:?}");
        let edges = edge_strokes(&b, &basis, e).unwrap();
        for l in &st {
            let (pa, pb) = (l.point(l.t0), l.point(l.t1));
            let (lo, hi) = if pa.z < pb.z { (pa, pb) } else { (pb, pa) };
            assert!(lo.z.abs() <= near, "{k:?}: unteres Ende z = {:e}", lo.z);
            assert!(
                plane(hi).abs() <= near,
                "{k:?}: oberes Ende {:e} neben der Schnittebene",
                plane(hi)
            );
            // Abstand zu einer gekrümmten Randkante (Ellipse), über den Kurvenparameter.
            let on_edge = edges
                .iter()
                .filter(|x| !matches!(x.curve, planora_kernel::Curve::Line { .. }))
                .map(|x| x.curve.point(x.curve.project(hi)).distance(hi))
                .fold(f64::INFINITY, f64::min);
            assert!(
                on_edge <= near,
                "{k:?}: oberes Ende {on_edge:e} neben der Kante"
            );
        }
    }
}

#[test]
fn degenerate_cone_seen_along_its_axis_has_no_silhouette() {
    // Kegel mit Steigung 0 (Mantel wie ein Zylinder) von oben: n·d ≡ 0, keine Linien.
    let s = flat_slope_cone_solid(5.0, 10.0);
    assert!(lines_of(&s, ViewKind::Top).is_empty());
    assert_eq!(lines_of(&s, ViewKind::Front).len(), 2);
}
