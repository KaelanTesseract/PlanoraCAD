use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

#[test]
fn overlapping_boxes_union_difference_intersection_are_valid() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::splat(5.0));
    for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        let r = k()
            .boolean(&a, &b, op)
            .unwrap_or_else(|e| panic!("{op:?}: {e}"));
        let report = validate(&r).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        assert_eq!(report.genus, 0, "{op:?}");
        assert_eq!(r.shells.len(), 1, "{op:?}");
    }
}

#[test]
fn contained_box_gives_a_cavity_with_two_shells() {
    let a = cube(10.0);
    let b = moved(&cube(2.0), DVec3::splat(4.0));
    let r = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    assert_eq!(r.shells.len(), 2);
    validate(&r).unwrap();
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    assert_eq!(u.shells.len(), 1);
    let i = k().boolean(&a, &b, BoolOp::Intersection).unwrap();
    assert_eq!(i.shells.len(), 1);
    assert_eq!(i.faces.len(), 6);
}

#[test]
fn disjoint_boxes() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(30.0, 0.0, 0.0));
    assert_eq!(k().boolean(&a, &b, BoolOp::Union).unwrap().shells.len(), 2);
    assert_eq!(
        k().boolean(&a, &b, BoolOp::Difference).unwrap().faces.len(),
        6
    );
    assert_eq!(
        k().boolean(&a, &b, BoolOp::Intersection).unwrap_err(),
        BooleanError::EmptyResult
    );
}

#[test]
fn boxes_touching_at_a_face() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(10.0, 0.0, 0.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
    assert_eq!(
        k().boolean(&a, &b, BoolOp::Intersection).unwrap_err(),
        BooleanError::EmptyResult
    );
    let d = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    assert_eq!(d.faces.len(), 6);
}

#[test]
fn identical_operands() {
    let a = cube(10.0);
    assert_eq!(k().boolean(&a, &a, BoolOp::Union).unwrap().faces.len(), 6);
    assert_eq!(
        k().boolean(&a, &a, BoolOp::Intersection)
            .unwrap()
            .faces
            .len(),
        6
    );
    assert_eq!(
        k().boolean(&a, &a, BoolOp::Difference).unwrap_err(),
        BooleanError::EmptyResult
    );
}

#[test]
fn boxes_touching_only_at_an_edge_do_not_panic() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(10.0, 10.0, 0.0));
    let r = k().boolean(&a, &b, BoolOp::Union);
    // entweder ein gültiger Körper oder ein benannter Fehler – niemals eine Panik
    if let Ok(s) = r {
        let _ = validate(&s);
    }
}

#[test]
fn box_stacked_flush_on_a_box() {
    let a = cube(10.0);
    let small = k().make_box(DVec3::new(6.0, 6.0, 10.0)).unwrap();
    let b = moved(&small, DVec3::new(2.0, 2.0, 10.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
}

#[test]
fn invalid_input_is_rejected() {
    let a = cube(10.0);
    let mut bad = cube(10.0);
    let id = Id::<Face>::from_index(0);
    bad.faces[id].outer.half_edges[0].forward = !bad.faces[id].outer.half_edges[0].forward;
    assert!(matches!(
        k().boolean(&a, &bad, BoolOp::Union),
        Err(BooleanError::InvalidInput(_))
    ));
}

#[test]
fn validator_checks_euler_per_shell() {
    let a = cube(10.0);
    let b = moved(&cube(2.0), DVec3::splat(4.0));
    let r = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    let report = validate(&r).unwrap();
    assert_eq!(report.genus, 0);
    assert_eq!(report.faces, 12);
}

#[test]
fn boxes_touching_at_part_of_a_face_are_sewn_across_t_junctions() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(10.0, 5.0, 5.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
    let d = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    validate(&d).unwrap();
    let corner = moved(
        &k().make_box(DVec3::splat(5.0)).unwrap(),
        DVec3::new(0.0, 0.0, 10.0),
    );
    let u = k().boolean(&a, &corner, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
}

#[test]
fn coaxial_cylinder_stack_keeps_flush_faces_only_from_a() {
    let a = k().make_cylinder(5.0, 10.0).unwrap();
    let same = moved(&a, DVec3::new(0.0, 0.0, 10.0));
    let u = k().boolean(&a, &same, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    // Mantel A, Mantel B, Boden A, Deckel B: die bündigen Kreisflächen entfallen
    assert_eq!(u.faces.len(), 4);
    let small = moved(
        &k().make_cylinder(3.0, 10.0).unwrap(),
        DVec3::new(0.0, 0.0, 10.0),
    );
    let u = k().boolean(&a, &small, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
    assert_eq!(k().boolean(&a, &a, BoolOp::Union).unwrap().faces.len(), 3);
    assert_eq!(
        k().boolean(&a, &a, BoolOp::Difference).unwrap_err(),
        BooleanError::EmptyResult
    );
}

#[test]
fn shells_must_cover_every_face_once() {
    let mut s = cube(10.0);
    s.shells[0].faces.pop();
    assert!(matches!(
        validate(&s),
        Err(ValidationError::ShellMembership(_))
    ));
}
