//! Nachbarschaft, Kantenwinkel und konvex/konkav (Teilprojekt 8, Task 1).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Kante zwischen den Flächen mit den Rollen `a` und `b` (Reihenfolge egal).
fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let names: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            names.len() == 2
                && ((names[0] == a && names[1] == b) || (names[0] == b && names[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn extrude_polygon(points: &[DVec2], h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::polygon(points).unwrap(),
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

/// L-Profil: konkave senkrechte Kante bei (4, 4) zwischen „Wand 3“ und „Wand 4“.
fn l_profile() -> Solid {
    extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    )
}

#[test]
fn a_box_edge_has_two_sides_and_a_right_convex_angle() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let sides = edge_sides(&s, e, None).unwrap();
    assert_ne!(sides.a.face, sides.b.face);
    let angle = edge_angle(&s, e, &sides).unwrap();
    assert!(angle.convex);
    assert!(
        (angle.interior - PI / 2.0).abs() < 1e-12,
        "{}",
        angle.interior
    );
    // Richtungen in die Flächen hinein stehen senkrecht auf der Kante und zeigen ins Material.
    let edge = &s.edges[e];
    let (wa, wb) = inward_directions(&s, e, &sides, 0.5 * (edge.t0 + edge.t1));
    let dir = edge.curve.tangent(edge.t0).normalize();
    assert!(wa.dot(dir).abs() < 1e-12 && wb.dot(dir).abs() < 1e-12);
    let mid = DVec3::new(5.0, 0.0, 10.0);
    for w in [wa, wb] {
        let p = mid + w * 0.1;
        assert!(p.x > 0.0 && p.y >= -1e-12 && p.z <= 10.0 + 1e-12, "{p:?}");
    }
}

#[test]
fn the_first_face_decides_which_side_is_a() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let uses = edge_uses(&s, e);
    let first = uses[1].face;
    let sides = edge_sides(&s, e, Some(first)).unwrap();
    assert_eq!(sides.a.face, first);
    let unchanged = edge_sides(&s, e, None).unwrap();
    assert_eq!(unchanged.a.face, uses[0].face);
}

#[test]
fn the_inner_edge_of_an_l_profile_is_concave() {
    let s = l_profile();
    let e = edge_between(&s, "Wand 3", "Wand 4");
    let sides = edge_sides(&s, e, None).unwrap();
    let angle = edge_angle(&s, e, &sides).unwrap();
    assert!(!angle.convex);
    assert!(
        (angle.interior - 1.5 * PI).abs() < 1e-12,
        "{}",
        angle.interior
    );
    let outer = edge_between(&s, "Wand 1", "Wand 2");
    let sides = edge_sides(&s, outer, None).unwrap();
    assert!(edge_angle(&s, outer, &sides).unwrap().convex);
}

#[test]
fn seams_and_tangent_edges_are_named() {
    // Naht des Zylindermantels: dieselbe Fläche auf beiden Seiten.
    let c = k().make_cylinder(3.0, 5.0).unwrap();
    let seam = c
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .map(|(id, _)| id)
        .unwrap();
    assert_eq!(edge_sides(&c, seam, None), Err(BlendError::SeamEdge));
    // Langloch: Gerade geht tangential in den Bogen über.
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
    let slot = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&slot, "Wand 1", "Wand 2");
    let sides = edge_sides(&slot, e, None).unwrap();
    match edge_angle(&slot, e, &sides) {
        Err(BlendError::NearlyTangent { angle_deg }) => {
            assert!((angle_deg - 180.0).abs() < 1e-6, "{angle_deg}")
        }
        other => panic!("{other:?}"),
    }
    let msg = BlendError::NearlyTangent { angle_deg: 180.0 }.to_string();
    assert!(msg.contains("tangential") && msg.contains("180.0"), "{msg}");
}

#[test]
fn foreign_edges_and_vertices_are_reported() {
    let s = k().make_box(DVec3::ONE).unwrap();
    assert_eq!(
        edge_sides(&s, Id::from_index(999), None),
        Err(BlendError::EdgeNotFound)
    );
    // Jede Ecke eines Quaders hat drei Kanten und drei Flächen.
    for (v, _) in s.vertices.iter() {
        assert_eq!(vertex_edges(&s, v).len(), 3);
        assert_eq!(vertex_faces(&s, v).len(), 3);
    }
}
