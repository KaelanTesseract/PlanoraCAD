use planora_kernel::*;
use planora_math::*;

fn tri_solid() -> (Solid, Id<Edge>, Id<Edge>) {
    let mut s = Solid::new();
    let a = s.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    let c = s.add_vertex(DVec3::new(0.0, 1.0, 0.0));
    let line = |from: DVec3, to: DVec3| Curve::Line {
        origin: from,
        dir: (to - from).normalize(),
    };
    let e1 = s.add_edge(line(DVec3::ZERO, DVec3::X), 0.0, 1.0, a, b);
    let e2 = s.add_edge(line(DVec3::X, DVec3::Y), 0.0, 2f64.sqrt(), b, c);
    (s, e1, e2)
}

#[test]
fn half_edge_endpoints_follow_direction() {
    let (s, e1, _) = tri_solid();
    let fwd = HalfEdge {
        edge: e1,
        forward: true,
    };
    let bwd = HalfEdge {
        edge: e1,
        forward: false,
    };
    assert_eq!(s.half_edge_start(fwd), s.edges[e1].start);
    assert_eq!(s.half_edge_end(fwd), s.edges[e1].end);
    assert_eq!(s.half_edge_start(bwd), s.edges[e1].end);
    assert_eq!(s.half_edge_end(bwd), s.edges[e1].start);
}

#[test]
fn loop_reversed_flips_order_and_direction() {
    let (_, e1, e2) = tri_solid();
    let l = Loop {
        half_edges: vec![
            HalfEdge {
                edge: e1,
                forward: true,
            },
            HalfEdge {
                edge: e2,
                forward: true,
            },
        ],
    };
    let r = l.reversed();
    assert_eq!(
        r.half_edges,
        vec![
            HalfEdge {
                edge: e2,
                forward: false
            },
            HalfEdge {
                edge: e1,
                forward: false
            },
        ]
    );
    assert_eq!(r.reversed(), l);
}

#[test]
fn seal_puts_all_faces_into_one_shell() {
    let (mut s, e1, _) = tri_solid();
    let face = Face {
        surface: Surface::Plane {
            frame: Frame::WORLD,
        },
        same_sense: true,
        outer: Loop {
            half_edges: vec![HalfEdge {
                edge: e1,
                forward: true,
            }],
        },
        inner: vec![],
        patch: None,
        tag: None,
    };
    let f1 = s.add_face(face.clone());
    let f2 = s.add_face(face);
    s.seal();
    assert_eq!(s.shells.len(), 1);
    assert_eq!(s.shells[0].faces, vec![f1, f2]);
}
