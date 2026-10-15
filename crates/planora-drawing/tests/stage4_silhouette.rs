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

// ---------------------------------------------------------------------------------------------
// Schmale Aussparungen und Streifen, die der Umriss quert (feiner als die gleichmäßige Abtastung).

/// z-Bereiche der Umrissstriche auf der Mantellinie x = 5, y = 0 (aufsteigend sortiert).
fn ranges_at_plus_x(st: &[Stroke]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = st
        .iter()
        .filter_map(|l| {
            let (a, b) = (l.point(l.t0), l.point(l.t1));
            let on = |p: planora_math::DVec3| (p.x - 5.0).abs() < 1e-9 && p.y.abs() < 1e-9;
            (on(a) && on(b)).then(|| (a.z.min(b.z), a.z.max(b.z)))
        })
        .collect();
    out.sort_by(|p, q| p.0.total_cmp(&q.0));
    out
}

#[test]
fn outline_leaves_a_gap_at_a_narrow_notch() {
    // Langer Zylinder (h = 300) mit einer 1 mm bzw. 0,5 mm hohen Kerbe ab z = 150 (Abtastabstand
    // 300/255 ≈ 1,18 mm): Der Umriss bei x = 5 muss dort unterbrochen sein.
    for (z0, z1) in [(150.0, 151.0), (150.0, 150.5)] {
        let s = notched_cylinder(5.0, 300.0, z0, z1);
        let st = lines_of(&s, ViewKind::Front);
        let r = ranges_at_plus_x(&st);
        assert_eq!(r.len(), 2, "Kerbe {z0}–{z1}: {r:?}");
        assert!(r[0].0.abs() < 1e-9 && (r[0].1 - z0).abs() < 1e-9, "{r:?}");
        assert!(
            (r[1].0 - z1).abs() < 1e-9 && (r[1].1 - 300.0).abs() < 1e-9,
            "{r:?}"
        );
    }
}

#[test]
fn outline_view_has_no_line_across_a_narrow_notch() {
    // Ganze Ansicht (Probe aus der Abschlussprüfung): kein Stück auf x = 5 überbrückt die Kerbe,
    // oberhalb geht der Umriss sichtbar weiter.
    let s = notched_cylinder(5.0, 300.0, 150.0, 151.0);
    let b = vec![DrawBody {
        name: "K".into(),
        solid: &s,
    }];
    let v = planora_drawing::compute_view(&b, ViewKind::Front, &Progress::new()).unwrap();
    let mut above = false;
    for p in &v.pieces {
        let st = &v.strokes[p.stroke];
        let (a, c) = (st.point2(&v.basis, p.t0), st.point2(&v.basis, p.t1));
        if (a.x - 5.0).abs() > 1e-6 || (c.x - 5.0).abs() > 1e-6 {
            continue;
        }
        let (lo, hi) = (a.y.min(c.y), a.y.max(c.y));
        assert!(
            !(lo < 150.5 && hi > 150.5),
            "Stück {lo}–{hi} ({:?}) überbrückt die Kerbe",
            p.vis
        );
        if lo <= 200.0 && hi >= 200.0 && p.vis == planora_drawing::Visibility::Visible {
            above = true;
        }
    }
    assert!(above, "Umriss oberhalb der Kerbe fehlt");
}

#[test]
fn outline_keeps_a_narrow_strip_of_face() {
    // Gegenstück: Vom Mantel bleibt bei +X nur ein 0,5 mm hoher Streifen z ∈ [150, 150,5]
    // (dazu die Ränder z ∈ [0, 1] und [299, 300]); der Umriss muss dort einen Strich haben.
    let s = strip_cylinder(5.0);
    let r = ranges_at_plus_x(&lines_of(&s, ViewKind::Front));
    assert_eq!(r.len(), 3, "{r:?}");
    assert!(
        (r[1].0 - 150.0).abs() < 1e-9 && (r[1].1 - 150.5).abs() < 1e-9,
        "{r:?}"
    );
    assert!(r[0].0.abs() < 1e-9 && (r[0].1 - 1.0).abs() < 1e-9, "{r:?}");
    assert!(
        (r[2].0 - 299.0).abs() < 1e-9 && (r[2].1 - 300.0).abs() < 1e-9,
        "{r:?}"
    );
}

/// Mitte zwischen zwei gleichmäßigen Abtaststellen eines Vollkreises (256 je Umlauf).
fn between_samples(i: usize) -> f64 {
    std::f64::consts::TAU * (i as f64 + 0.5) / 256.0
}

/// Liegt der Kreisparameter von `p` im Bereich des Strichs (modulo 2π)?
fn circle_stroke_contains(l: &Stroke, p: planora_math::DVec3) -> bool {
    let t = l.curve.project(p);
    let t = l.t0 + (t - l.t0).rem_euclid(std::f64::consts::TAU);
    t <= l.t1
}

#[test]
fn sphere_outline_has_a_gap_at_a_small_hole() {
    // Kugel r = 10 von vorne: Großkreis in der XZ-Ebene (eigene Rechnung: e1 = X, e2 = −Z).
    // Kleine radiale Bohrung (r = 0,05) genau zwischen zwei Abtaststellen (Abstand ≈ 0,245 mm).
    let (big, hr) = (10.0, 0.05);
    let t = between_samples(32);
    let dir = planora_math::DVec3::new(t.cos(), 0.0, -t.sin());
    let s = sphere_with_radial_hole(big, hr, dir);
    let st: Vec<Stroke> = lines_of(&s, ViewKind::Front)
        .into_iter()
        .filter(|l| {
            matches!(l.curve, planora_kernel::Curve::Circle { radius, .. }
                if (radius - big).abs() < 1e-9)
        })
        .collect();
    let center = dir * big;
    for l in &st {
        assert!(
            !circle_stroke_contains(l, center),
            "Umriss quert die Bohrung"
        );
    }
    let total: f64 = st.iter().map(|l| (l.t1 - l.t0) * big).sum();
    let gap = 2.0 * big * (hr / big).asin();
    assert!(
        (total - (std::f64::consts::TAU * big - gap)).abs() < 1e-6,
        "{total}"
    );
}

#[test]
fn torus_fillet_outline_has_a_gap_at_a_small_window() {
    // Gerundeter Zylinder (Rundung R = 7, r = 3, Mitte z = 7) von vorne: der Umriss der Rundung
    // ist der Meridian-Viertelkreis in der Ebene y = 0 bei +X, P(t) = (7 + 3 cos t, 0, 7 + 3 sin t),
    // t ∈ [0, π/2] (eigene Rechnung). Ein Fenster z ∈ [z_c ± 0,02] um u = 0 mitten zwischen zwei
    // Abtaststellen (Abstand 3·2π/256 ≈ 0,074 mm) muss eine Lücke t ∈ [asin((z_c − 0,02 − 7)/3),
    // asin((z_c + 0,02 − 7)/3)] geben.
    let hz = 0.02;
    let tc = between_samples(32);
    let zc = 7.0 + 3.0 * tc.sin();
    let (s, face) = rounded_cylinder_with_window(0.0, 0.01, zc, hz);
    let st: Vec<Stroke> = lines_of(&s, ViewKind::Front)
        .into_iter()
        .filter(|l| {
            l.origin == StrokeOrigin::Silhouette { body: 0, face }
                && matches!(&l.curve, planora_kernel::Curve::Circle { frame, radius }
                    if (radius - 3.0).abs() < 1e-9 && frame.origin.x > 0.0)
        })
        .collect();
    assert!(!st.is_empty());
    let p = planora_math::DVec3::new(7.0 + 3.0 * tc.cos(), 0.0, zc);
    for l in &st {
        assert!(!circle_stroke_contains(l, p), "Umriss quert das Fenster");
    }
    let total: f64 = st.iter().map(|l| (l.t1 - l.t0) * 3.0).sum();
    let gap = 3.0 * (((zc + hz - 7.0) / 3.0).asin() - ((zc - hz - 7.0) / 3.0).asin());
    assert!(
        (total - (std::f64::consts::FRAC_PI_2 * 3.0 - gap)).abs() < 1e-6,
        "{total}"
    );
}

#[test]
fn torus_fillet_spline_outline_has_a_gap_at_a_small_window() {
    // Dieselbe Rundung isometrisch (Spline-Umriss). Umrisspunkt nach eigener Rechnung: Normale
    // n(u, v) = (cos v cos u, cos v sin u, sin v) im Rahmen der Rundung (um 20° gedreht), n·d = 0
    // → tan v = −(d_x cos u + d_y sin u)/d_z; u mitten zwischen zwei Abtaststellen, v in der
    // Rundung. Fenster dort: ±0,05 mm in z und längs des Umfangs; kein Umrisspunkt darf näher
    // als 0,03 mm an den Fenstermittelpunkt herankommen.
    let d = Basis::of(ViewKind::Isometric).d;
    let a = 20f64.to_radians();
    let fx = planora_math::DVec3::new(a.cos(), a.sin(), 0.0);
    let fy = planora_math::DVec3::new(-a.sin(), a.cos(), 0.0);
    let (dx, dy, dz) = (d.dot(fx), d.dot(fy), d.z);
    let (u, v) = (0..256)
        .find_map(|i| {
            let u = between_samples(i);
            let v1 = (-(dx * u.cos() + dy * u.sin())).atan2(dz);
            [v1, v1 + std::f64::consts::PI]
                .into_iter()
                .map(|v| v.rem_euclid(std::f64::consts::TAU))
                .find(|&v| v > 0.4 && v < 1.1)
                .map(|v| (u, v))
        })
        .expect("Umrissstelle auf der Rundung");
    let radial = fx * u.cos() + fy * u.sin();
    let rho = 7.0 + 3.0 * v.cos();
    let p = radial * rho + planora_math::DVec3::Z * (7.0 + 3.0 * v.sin());
    let normal = radial * v.cos() + planora_math::DVec3::Z * v.sin();
    assert!(normal.dot(d).abs() < 1e-12);
    let (s, face) = rounded_cylinder_with_window(u + a, 0.05 / rho, p.z, 0.05);
    let st = lines_of(&s, ViewKind::Isometric);
    let mut seen = false;
    for l in st
        .iter()
        .filter(|l| l.origin == StrokeOrigin::Silhouette { body: 0, face })
    {
        seen = true;
        for i in 0..=8000 {
            let q = l.point(l.t0 + (l.t1 - l.t0) * i as f64 / 8000.0);
            assert!(q.distance(p) > 0.03, "Umriss quert das Fenster bei {q:?}");
        }
    }
    assert!(seen);
}
