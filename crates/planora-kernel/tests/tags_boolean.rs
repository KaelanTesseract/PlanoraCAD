use planora_kernel::*;
use planora_math::{DMat4, DVec3, Tolerance};
use std::collections::HashSet;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn boxed(size: DVec3, at: DVec3, origin: u64) -> Solid {
    let mut s = k().make_box(size).unwrap();
    s = k().transform(&s, &DMat4::from_translation(at)).unwrap();
    s.assign_origin(origin);
    s
}

fn origins(s: &Solid) -> HashSet<u64> {
    s.faces
        .iter()
        .map(|(_, f)| f.tag.as_ref().expect("jede Fläche trägt eine Marke").origin)
        .collect()
}

#[test]
fn every_result_face_keeps_the_tag_of_its_source_face() {
    let a = boxed(DVec3::splat(10.0), DVec3::ZERO, 1);
    let b = boxed(DVec3::splat(10.0), DVec3::splat(5.0), 2);
    for (op, want) in [
        (BoolOp::Union, HashSet::from([1, 2])),
        (BoolOp::Difference, HashSet::from([1, 2])),
        (BoolOp::Intersection, HashSet::from([1, 2])),
    ] {
        let r = k().boolean(&a, &b, op).unwrap();
        assert_eq!(origins(&r), want, "{op:?}");
        k().validate(&r).unwrap();
    }
}

#[test]
fn flipped_tool_faces_keep_their_tag_and_a_split_face_keeps_one_tag_for_all_pieces() {
    let plate = boxed(DVec3::new(20.0, 20.0, 5.0), DVec3::ZERO, 1);
    // Nut quer über die ganze Breite: zerteilt die Deckfläche in zwei Teile
    let groove = boxed(DVec3::new(4.0, 30.0, 2.0), DVec3::new(8.0, -5.0, 3.0), 2);
    let r = k().boolean(&plate, &groove, BoolOp::Difference).unwrap();
    let lid = FaceTag {
        origin: 1,
        role: "Deckel".into(),
    };
    assert_eq!(
        r.faces_with_tag(&lid).len(),
        2,
        "die Deckfläche wurde in zwei Stücke geteilt"
    );
    // die Nutwände stammen vom Werkzeug (origin 2), umgedreht
    assert!(
        r.faces
            .iter()
            .any(|(_, f)| f.tag.as_ref().is_some_and(|t| t.origin == 2))
    );
}

#[test]
fn flush_and_curved_cases_keep_their_tags() {
    let a = boxed(DVec3::new(10.0, 10.0, 5.0), DVec3::ZERO, 1);
    let mut cyl = k().make_cylinder(2.0, 5.0).unwrap();
    cyl = k()
        .transform(&cyl, &DMat4::from_translation(DVec3::new(5.0, 5.0, 0.0)))
        .unwrap();
    cyl.assign_origin(2);
    let r = k().boolean(&a, &cyl, BoolOp::Difference).unwrap();
    assert!(r.faces.iter().all(|(_, f)| f.tag.is_some()));
    let bore = FaceTag {
        origin: 2,
        role: "Wand 1".into(),
    };
    assert_eq!(
        r.faces_with_tag(&bore).len(),
        1,
        "die Bohrungswand ist eine Fläche"
    );
}

#[test]
fn probe_points_lie_on_their_face_or_edge() {
    let s = boxed(DVec3::new(4.0, 6.0, 8.0), DVec3::new(1.0, 2.0, 3.0), 1);
    let tol = Tolerance::default();
    for (id, f) in s.faces.iter() {
        let p = face_probe_point(&s, id, &tol).expect("Prüfpunkt");
        assert!(
            f.surface.implicit(p).abs() < 1e-9,
            "Punkt liegt auf der Trägerfläche"
        );
    }
    for (id, e) in s.edges.iter() {
        let p = edge_probe_point(&s, id).unwrap();
        assert!((p - e.curve.point(0.5 * (e.t0 + e.t1))).length() < 1e-12);
    }
    assert!(face_probe_point(&s, Id::from_index(99), &tol).is_none());
    assert!(edge_probe_point(&s, Id::from_index(99)).is_none());
}

#[test]
fn probe_points_exist_for_curved_faces() {
    let s = k().make_cylinder(3.0, 4.0).unwrap();
    for (id, f) in s.faces.iter() {
        let p = face_probe_point(&s, id, &Tolerance::default()).expect("Prüfpunkt");
        assert!(f.surface.implicit(p).abs() < 1e-9);
    }
}

#[test]
fn probe_points_exist_for_faces_with_poles_and_lie_strictly_inside() {
    let tol = Tolerance::default();
    for s in [
        k().make_sphere(2.5).unwrap(),
        k().make_cone(2.0, 3.0).unwrap(),
        k().make_cylinder(3.0, 4.0).unwrap(),
    ] {
        for (id, f) in s.faces.iter() {
            let p = face_probe_point(&s, id, &tol).expect("Prüfpunkt");
            assert!(f.surface.implicit(p).abs() < 1e-9);
            let g = boolean::face_geom(&s, id);
            assert_eq!(
                boolean::point_in_face(&g, p, &tol),
                boolean::Containment::Inside
            );
        }
    }
}
