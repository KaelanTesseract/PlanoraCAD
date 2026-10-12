use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn roles(s: &Solid) -> Vec<String> {
    let mut v: Vec<String> = s
        .faces
        .iter()
        .map(|(_, f)| f.tag.as_ref().map(|t| t.role.clone()).unwrap_or_default())
        .collect();
    v.sort();
    v
}

fn outward_normal(s: &Solid, role: &str) -> DVec3 {
    let (_, f) = s
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        .unwrap_or_else(|| panic!("Rolle {role} fehlt"));
    let n = f.surface.normal(0.0, 0.0);
    if f.same_sense { n } else { -n }
}

#[test]
fn a_box_carries_floor_ceiling_and_four_walls() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    assert_eq!(
        roles(&s),
        ["Boden", "Deckel", "Wand 1", "Wand 2", "Wand 3", "Wand 4"]
    );
    assert!(
        s.faces
            .iter()
            .all(|(_, f)| f.tag.as_ref().unwrap().origin == 0)
    );
    k().validate(&s).unwrap();
}

#[test]
fn wall_numbers_follow_the_input_order_of_the_profile() {
    let rect = |pts: [DVec2; 4]| Profile {
        outer: ProfileLoop::polygon(&pts).unwrap(),
        holes: vec![],
    };
    let (a, b, c, d) = (
        DVec2::new(0.0, 0.0),
        DVec2::new(4.0, 0.0),
        DVec2::new(4.0, 3.0),
        DVec2::new(0.0, 3.0),
    );
    // gegen den Uhrzeigersinn: Segment 1 ist die untere Kante (a→b), Wand zeigt nach −y
    let ccw = k()
        .extrude(&rect([a, b, c, d]), &planora_math::Frame::WORLD, 2.0)
        .unwrap();
    assert!((outward_normal(&ccw, "Wand 1") - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
    // im Uhrzeigersinn eingegeben (wird intern umgedreht): Segment 1 ist a→d, Wand zeigt nach −x
    let cw = k()
        .extrude(&rect([a, d, c, b]), &planora_math::Frame::WORLD, 2.0)
        .unwrap();
    assert!((outward_normal(&cw, "Wand 1") - DVec3::new(-1.0, 0.0, 0.0)).length() < 1e-9);
    assert!((outward_normal(&cw, "Wand 2") - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
}

#[test]
fn holes_get_their_own_numbering() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![
            ProfileLoop::circle(DVec2::new(5.0, 5.0), 2.0),
            ProfileLoop::circle(DVec2::new(14.0, 14.0), 3.0),
        ],
    };
    let s = k().extrude(&p, &planora_math::Frame::WORLD, 3.0).unwrap();
    let r = roles(&s);
    for want in [
        "Boden",
        "Deckel",
        "Wand 1",
        "Wand 4",
        "Loch 1, Wand 1",
        "Loch 2, Wand 1",
    ] {
        assert!(r.iter().any(|x| x == want), "{want} fehlt in {r:?}");
    }
    assert_eq!(r.len(), 8);
}

#[test]
fn cylinder_cone_and_sphere_roles() {
    assert_eq!(
        roles(&k().make_cylinder(2.0, 5.0).unwrap()),
        ["Boden", "Deckel", "Wand 1"]
    );
    assert_eq!(
        roles(&k().make_cone(2.0, 5.0).unwrap()),
        ["Fläche 1", "Fläche 2"]
    );
    assert_eq!(roles(&k().make_sphere(3.0).unwrap()), ["Fläche 1"]);
}

#[test]
fn revolve_roles_follow_the_input_order_of_the_chain() {
    let up = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 5.0),
            },
        ],
    };
    let a = k().revolve(&up, &planora_math::Frame::WORLD).unwrap();
    let kind = |s: &Solid, role: &str| {
        let (id, _) = s
            .faces
            .iter()
            .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
            .unwrap();
        k().face_kind(s, id).unwrap()
    };
    assert_eq!(
        kind(&a, "Fläche 1"),
        SurfaceKind::Plane,
        "Segment 1 ist die Grundscheibe"
    );
    assert_eq!(kind(&a, "Fläche 2"), SurfaceKind::Cone);
    let down = up.try_reversed().unwrap();
    let b = k().revolve(&down, &planora_math::Frame::WORLD).unwrap();
    assert_eq!(
        kind(&b, "Fläche 1"),
        SurfaceKind::Cone,
        "umgekehrt eingegeben: Segment 1 ist der Mantel"
    );
    assert_eq!(kind(&b, "Fläche 2"), SurfaceKind::Plane);
}

#[test]
fn assign_origin_only_fills_unassigned_tags_and_faces_are_found_by_tag() {
    let mut s = k().make_box(DVec3::splat(2.0)).unwrap();
    s.assign_origin(7);
    assert!(
        s.faces
            .iter()
            .all(|(_, f)| f.tag.as_ref().unwrap().origin == 7)
    );
    s.assign_origin(9);
    assert!(
        s.faces
            .iter()
            .all(|(_, f)| f.tag.as_ref().unwrap().origin == 7),
        "bereits zugewiesen"
    );
    let tag = FaceTag {
        origin: 7,
        role: "Deckel".into(),
    };
    assert_eq!(s.faces_with_tag(&tag).len(), 1);
    assert!(
        s.faces_with_tag(&FaceTag {
            origin: 8,
            role: "Deckel".into()
        })
        .is_empty()
    );
}

#[test]
fn transforms_keep_the_tags() {
    let mut s = k().make_box(DVec3::splat(2.0)).unwrap();
    s.assign_origin(3);
    let moved = k()
        .transform(&s, &DMat4::from_translation(DVec3::new(5.0, 0.0, 0.0)))
        .unwrap();
    assert_eq!(roles(&moved), roles(&s));
    assert!(
        moved
            .faces
            .iter()
            .all(|(_, f)| f.tag.as_ref().unwrap().origin == 3)
    );
    let mirrored = k()
        .transform(&s, &DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)))
        .unwrap();
    assert_eq!(roles(&mirrored), roles(&s));
}
