//! Bahn mit Hautfläche: variabler Radius, Übereinstimmung mit der analytischen Rundung,
//! Grenzen (Teilprojekt 8, Task 14).

use planora_kernel::blend::analytic::{SectionKind, analytic_geom};
use planora_kernel::blend::chain::select_edges;
use planora_kernel::blend::march::*;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn req(e: Id<Edge>, kind: BlendKind) -> BlendRequest {
    BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind,
        tangent_chain: false,
    }
}

#[test]
fn a_constant_march_reproduces_the_analytic_cylinder() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let r = req(e, BlendKind::Fillet { radius: 2.0 });
    let ce = select_edges(&s, &r).unwrap()[0];
    let sides = edge_sides(&s, e, None).unwrap();
    let tol = Tolerance::default();
    let m = march_geom(&s, &ce, &sides, true, SectionLaw::Fillet(2.0), &tol).unwrap();
    let a = analytic_geom(
        &s,
        e,
        &sides,
        true,
        SectionKind::Fillet { radius: 2.0 },
        &tol,
    )
    .unwrap()
    .unwrap();
    assert!(!m.analytic && matches!(m.surface, Surface::Nurbs(_)));
    for i in 0..=20 {
        let f = i as f64 / 20.0;
        let pm = m.contact_a.point(m.range.0 + (m.range.1 - m.range.0) * f);
        let pa = a.contact_a.point(a.range.0 + (a.range.1 - a.range.0) * f);
        assert!(pm.distance(pa) < 1e-6, "{pm:?} / {pa:?}");
    }
}

#[test]
fn a_variable_radius_on_a_box_edge() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k()
        .blend(
            &s,
            &req(
                e,
                BlendKind::VariableFillet {
                    points: vec![(0.0, 1.0), (1.0, 2.0)],
                },
            ),
        )
        .unwrap();
    validate(&b.solid).unwrap();
    // Zwei Punkte: das Gesetz ist linear, r(s) = 1 + s/10; abgetragen wird
    // (1 − π/4) · ∫₀¹⁰ (1 + s/10)² ds = (1 − π/4) · 10 · 7/3.
    let want = -(1.0 - PI / 4.0) * 10.0 * 7.0 / 3.0;
    assert!(
        (b.volume_change - want).abs() < 3e-3 * want.abs(),
        "{} statt {want}",
        b.volume_change
    );
}

#[test]
fn closed_edges_need_equal_radii_at_the_seam() {
    let c = k().make_cylinder(5.0, 8.0).unwrap();
    let e = edge_between(&c, "Wand 1", "Deckel");
    let r = k().blend(
        &c,
        &req(
            e,
            BlendKind::VariableFillet {
                points: vec![(0.0, 1.0), (1.0, 1.5)],
            },
        ),
    );
    assert!(
        matches!(&r, Err(BlendError::InvalidParameter(m)) if m.contains("geschlossenen")),
        "{:?}",
        r.map(|_| ())
    );
    let ok = k()
        .blend(
            &c,
            &req(
                e,
                BlendKind::VariableFillet {
                    points: vec![(0.0, 1.0), (0.5, 1.5), (1.0, 1.0)],
                },
            ),
        )
        .unwrap();
    validate(&ok.solid).unwrap();
}

#[test]
fn oblique_ends_of_a_skin_are_named() {
    let tri = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(5.0, 75f64.sqrt()),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            4.0,
        )
        .unwrap();
    let e = edge_between(&tri, "Deckel", "Wand 1");
    match k().blend(
        &tri,
        &req(
            e,
            BlendKind::VariableFillet {
                points: vec![(0.0, 0.5), (1.0, 0.8)],
            },
        ),
    ) {
        Err(BlendError::Unsupported(m)) => assert_eq!(m, OBLIQUE_END),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

// ---- Zusätzliche Tests (über den Plan hinaus) ----

#[test]
fn hostile_and_freeform_laws_are_named() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    // Positionen so dicht, dass die Steigung nicht endlich wäre: benannt, keine Panik.
    let r = k().blend(
        &s,
        &req(
            e,
            BlendKind::VariableFillet {
                points: vec![(0.0, 1.0), (1e-320, 2.0)],
            },
        ),
    );
    assert!(
        matches!(&r, Err(BlendError::InvalidParameter(m)) if m.contains("Positionen")),
        "{:?}",
        r.map(|_| ())
    );
    // Variabler Radius an Freiformflächen: erst mit Task 15 geprüft, bis dahin benannt.
    let c = planora_math::nurbs::NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        0.0,
        planora_math::TAU,
    )
    .unwrap();
    let ell = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::nurbs_loop(c).unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            4.0,
        )
        .unwrap();
    let rim = edge_between(&ell, "Wand 1", "Deckel");
    match k().blend(
        &ell,
        &req(
            rim,
            BlendKind::VariableFillet {
                points: vec![(0.0, 0.5), (0.5, 0.7), (1.0, 0.5)],
            },
        ),
    ) {
        Err(BlendError::Unsupported(m)) => {
            assert_eq!(m, planora_kernel::blend::VARIABLE_FREEFORM)
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn the_march_keeps_the_section_on_both_faces_and_the_ends_exact() {
    // Konkave Kante (L-Profil innen): Material kommt hinzu; Querschnitte an den Enden sind exakte
    // Kreisbögen in den Deckel- und Bodenebenen.
    let l = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(10.0, 4.0),
                    DVec2::new(4.0, 4.0),
                    DVec2::new(4.0, 10.0),
                    DVec2::new(0.0, 10.0),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            5.0,
        )
        .unwrap();
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let sides = edge_sides(&l, e, None).unwrap();
    assert!(!edge_angle(&l, e, &sides).unwrap().convex);
    let law = planora_math::RadiusLaw::new(&[(0.0, 0.5), (0.6, 1.2), (1.0, 0.8)]).unwrap();
    let r = req(
        e,
        BlendKind::VariableFillet {
            points: vec![(0.0, 0.5), (0.6, 1.2), (1.0, 0.8)],
        },
    );
    let ce = select_edges(&l, &r).unwrap()[0];
    let tol = Tolerance::default();
    let g = march_geom(&l, &ce, &sides, false, SectionLaw::Variable(&law), &tol).unwrap();
    assert!(!g.analytic && !g.convex);
    assert!((g.range.1 - 5.0).abs() < 1e-12, "{:?}", g.range);
    let (fa, fb) = (&l.faces[sides.a.face], &l.faces[sides.b.face]);
    for i in 0..=100 {
        let x = 5.0 * i as f64 / 100.0;
        assert!(fa.surface.implicit(g.contact_a.point(x)).abs() < 1e-5);
        assert!(fb.surface.implicit(g.contact_b.point(x)).abs() < 1e-5);
    }
    for (seg, z) in [(&g.section_start, 0.0), (&g.section_end, 5.0)] {
        for k in 0..=10 {
            let p = seg.at(k as f64 / 10.0);
            assert!((p.z - z).abs() < 1e-9, "{p}");
        }
    }
    let b = k().blend(&l, &r).unwrap();
    validate(&b.solid).unwrap();
    assert!(b.volume_change > 0.0);
}
