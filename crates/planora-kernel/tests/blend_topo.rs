//! Nachbarschaft, Kantenwinkel und konvex/konkav (Teilprojekt 8, Task 1).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

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

fn approx(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
}

/// Kante, die zwischen den benannten Flächen liegt und deren Kurve ein Kreis ist.
fn circle_edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = &s.edges[id];
            let names: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            !matches!(e.curve, Curve::Line { .. })
                && names.len() == 2
                && ((names[0] == a && names[1] == b) || (names[0] == b && names[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kreiskante zwischen {a} und {b}"))
}

fn angle_of(s: &Solid, e: Id<Edge>) -> EdgeAngle {
    let sides = edge_sides(s, e, None).unwrap();
    edge_angle(s, e, &sides).unwrap()
}

#[test]
fn inward_directions_are_pinned_to_the_faces() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let edge = &s.edges[e];
    let t = 0.5 * (edge.t0 + edge.t1);
    for first in [None, Some(edge_uses(&s, e)[1].face)] {
        let sides = edge_sides(&s, e, first).unwrap();
        let (wa, wb) = inward_directions(&s, e, &sides, t);
        let (top, wall) = if face_name(&s, sides.a.face) == "Deckel" {
            (wa, wb)
        } else {
            (wb, wa)
        };
        assert!(approx(top, DVec3::Y), "{top:?}");
        assert!(approx(wall, -DVec3::Z), "{wall:?}");
        let a = edge_angle(&s, e, &sides).unwrap();
        assert!(a.convex && (a.interior - PI / 2.0).abs() < 1e-12);
    }
    // Tauschen der ersten Fläche tauscht A und B und lässt Winkel und Konvexität unverändert.
    let s1 = edge_sides(&s, e, None).unwrap();
    let s2 = edge_sides(&s, e, Some(s1.b.face)).unwrap();
    assert_eq!((s2.a, s2.b), (s1.b, s1.a));
    assert_eq!(
        edge_angle(&s, e, &s1).unwrap(),
        edge_angle(&s, e, &s2).unwrap()
    );
    let (a1, b1) = inward_directions(&s, e, &s1, t);
    let (a2, b2) = inward_directions(&s, e, &s2, t);
    assert!(approx(a1, b2) && approx(b1, a2));
}

#[test]
fn inward_directions_of_a_concave_edge_point_into_the_material_sides() {
    // Konkave Kante des L-Profils bei (4, 4): Wand 3 liegt bei y = 4 (Innenseite zeigt nach -y),
    // Wand 4 bei x = 4 (Innenseite der Fläche zeigt nach +x).
    let s = l_profile();
    let e = edge_between(&s, "Wand 3", "Wand 4");
    let edge = &s.edges[e];
    let sides = edge_sides(&s, e, None).unwrap();
    let (wa, wb) = inward_directions(&s, e, &sides, 0.5 * (edge.t0 + edge.t1));
    let (w3, w4) = if face_name(&s, sides.a.face) == "Wand 3" {
        (wa, wb)
    } else {
        (wb, wa)
    };
    assert!(approx(w3, DVec3::X), "{w3:?}");
    assert!(approx(w4, DVec3::Y), "{w4:?}");
}

#[test]
fn a_stale_first_face_is_refused() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let adjacent: Vec<_> = edge_uses(&s, e).iter().map(|u| u.face).collect();
    let other = s
        .faces
        .iter()
        .map(|(id, _)| id)
        .find(|id| !adjacent.contains(id))
        .unwrap();
    assert!(matches!(
        edge_sides(&s, e, Some(other)),
        Err(BlendError::InvalidInput(m)) if m.contains("grenzt nicht")
    ));
}

#[test]
fn a_cylinder_rim_is_convex_at_a_right_angle() {
    let c = k().make_cylinder(3.0, 5.0).unwrap();
    for cap in ["Boden", "Deckel"] {
        let a = angle_of(&c, circle_edge_between(&c, "Wand 1", cap));
        assert!(a.convex, "{cap}");
        assert!(
            (a.interior - PI / 2.0).abs() < 1e-9,
            "{cap}: {}",
            a.interior
        );
    }
}

#[test]
fn a_hole_rim_is_convex_and_uses_a_reversed_face() {
    // Quader minus Zylinder: die Lochwand hat same_sense == false.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let cy = k().make_cylinder(2.0, 20.0).unwrap();
    let cy = k()
        .transform(&cy, &DMat4::from_translation(DVec3::new(5.0, 5.0, -5.0)))
        .unwrap();
    let hole = k().boolean(&b, &cy, BoolOp::Difference).unwrap();
    assert!(
        hole.faces
            .iter()
            .any(|(_, f)| !f.same_sense && f.tag.as_ref().is_some_and(|t| t.role == "Wand 1"))
    );
    for cap in ["Deckel", "Boden"] {
        let e = hole
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let u = edge_uses(&hole, id);
                u.len() == 2
                    && u.iter().any(|x| {
                        !hole.faces[x.face].same_sense && face_name(&hole, x.face) == "Wand 1"
                    })
                    && u.iter().any(|x| face_name(&hole, x.face) == cap)
            })
            .unwrap();
        let a = angle_of(&hole, e);
        // Der Rand eines Lochs ist eine Außenkante des Materials (90°), also konvex.
        assert!(a.convex, "{cap}");
        assert!(
            (a.interior - PI / 2.0).abs() < 1e-9,
            "{cap}: {}",
            a.interior
        );
        // Auch mit vertauschter Reihenfolge dasselbe Ergebnis.
        let sides = edge_sides(&hole, e, None).unwrap();
        let swapped = edge_sides(&hole, e, Some(sides.b.face)).unwrap();
        assert_eq!(edge_angle(&hole, e, &swapped).unwrap(), a);
    }
}

#[test]
fn a_boss_has_a_concave_foot_and_a_convex_rim() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let cy = k().make_cylinder(2.0, 4.0).unwrap();
    let cy = k()
        .transform(&cy, &DMat4::from_translation(DVec3::new(5.0, 5.0, 10.0)))
        .unwrap();
    let boss = k().boolean(&b, &cy, BoolOp::Union).unwrap();
    let mut convex = 0;
    let mut concave = 0;
    for (id, e) in boss.edges.iter() {
        if matches!(e.curve, Curve::Line { .. }) {
            continue;
        }
        let u = edge_uses(&boss, id);
        if u.len() != 2 || u[0].face == u[1].face {
            continue;
        }
        let a = angle_of(&boss, id);
        if a.convex {
            assert!((a.interior - PI / 2.0).abs() < 1e-9);
            convex += 1;
        } else {
            assert!((a.interior - 1.5 * PI).abs() < 1e-9);
            concave += 1;
        }
    }
    assert_eq!((convex, concave), (1, 1));
}

#[test]
fn a_cone_rim_has_the_slope_angle() {
    let c = k().make_cone(3.0, 5.0).unwrap();
    let rim = c
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let u = edge_uses(&c, id);
            u.len() == 2 && u[0].face != u[1].face
        })
        .unwrap();
    let a = angle_of(&c, rim);
    assert!(a.convex);
    // Zwischen Grundfläche und Mantel liegt der Böschungswinkel atan(h / r).
    assert!(
        (a.interior - (5.0_f64).atan2(3.0)).abs() < 1e-9,
        "{}",
        a.interior
    );
}

#[test]
fn the_sphere_seam_is_named() {
    let s = k().make_sphere(3.0).unwrap();
    let (id, _) = s.edges.iter().next().unwrap();
    assert_eq!(edge_sides(&s, id, None), Err(BlendError::SeamEdge));
}
