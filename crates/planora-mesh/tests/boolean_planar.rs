use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

fn vol(s: &Solid) -> f64 {
    let m = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.001,
        },
    )
    .unwrap();
    assert!(is_watertight(&m, 1e-6), "Netz nicht geschlossen");
    mass_properties(&m).volume
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0), "{a} != {b}");
}

#[test]
fn overlapping_boxes_have_the_expected_volumes() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::splat(5.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    let d = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    let i = k().boolean(&a, &b, BoolOp::Intersection).unwrap();
    close(vol(&u), 1875.0);
    close(vol(&d), 875.0);
    close(vol(&i), 125.0);
    close(vol(&u) + vol(&i), vol(&a) + vol(&b));
    close(
        vol(&k().boolean(&b, &a, BoolOp::Difference).unwrap()),
        875.0,
    );
}

#[test]
fn box_with_a_cavity_and_contained_box() {
    let a = cube(10.0);
    let b = moved(&cube(2.0), DVec3::splat(4.0));
    close(
        vol(&k().boolean(&a, &b, BoolOp::Difference).unwrap()),
        992.0,
    );
    close(vol(&k().boolean(&a, &b, BoolOp::Union).unwrap()), 1000.0);
    close(
        vol(&k().boolean(&a, &b, BoolOp::Intersection).unwrap()),
        8.0,
    );
}

#[test]
fn through_hole_by_a_box() {
    let a = cube(10.0);
    let tool = moved(
        &k().make_box(DVec3::new(4.0, 4.0, 12.0)).unwrap(),
        DVec3::new(3.0, 3.0, -1.0),
    );
    let d = k().boolean(&a, &tool, BoolOp::Difference).unwrap();
    close(vol(&d), 1000.0 - 160.0);
    assert_eq!(validate(&d).unwrap().genus, 1);
}

#[test]
fn flush_and_touching_boxes() {
    let a = cube(10.0);
    let on_top = moved(
        &k().make_box(DVec3::new(6.0, 6.0, 10.0)).unwrap(),
        DVec3::new(2.0, 2.0, 10.0),
    );
    close(
        vol(&k().boolean(&a, &on_top, BoolOp::Union).unwrap()),
        1360.0,
    );
    let next_to = moved(&cube(10.0), DVec3::new(10.0, 0.0, 0.0));
    close(
        vol(&k().boolean(&a, &next_to, BoolOp::Union).unwrap()),
        2000.0,
    );
    close(
        vol(&k().boolean(&a, &next_to, BoolOp::Difference).unwrap()),
        1000.0,
    );
}

#[test]
fn identical_operands_volumes() {
    let a = cube(10.0);
    close(vol(&k().boolean(&a, &a, BoolOp::Union).unwrap()), 1000.0);
    close(
        vol(&k().boolean(&a, &a, BoolOp::Intersection).unwrap()),
        1000.0,
    );
}

#[test]
fn volume_plausibility_helper_accepts_and_rejects() {
    // implementiert in Task 9; hier nur die Bilanz als Vorgriff
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::splat(5.0));
    let (va, vb) = (vol(&a), vol(&b));
    let u = vol(&k().boolean(&a, &b, BoolOp::Union).unwrap());
    assert!(u >= va.max(vb) - 1e-6 && u <= va + vb + 1e-6);
}

#[test]
fn chained_operations() {
    let a = cube(20.0);
    let h1 = moved(
        &k().make_box(DVec3::new(4.0, 4.0, 24.0)).unwrap(),
        DVec3::new(3.0, 3.0, -2.0),
    );
    let h2 = moved(
        &k().make_box(DVec3::new(4.0, 4.0, 24.0)).unwrap(),
        DVec3::new(12.0, 12.0, -2.0),
    );
    let step1 = k().boolean(&a, &h1, BoolOp::Difference).unwrap();
    let step2 = k().boolean(&step1, &h2, BoolOp::Difference).unwrap();
    close(vol(&step2), 8000.0 - 2.0 * 16.0 * 20.0);
    assert_eq!(validate(&step2).unwrap().genus, 2);
}

#[test]
fn partially_touching_boxes() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(10.0, 5.0, 5.0));
    close(vol(&k().boolean(&a, &b, BoolOp::Union).unwrap()), 2000.0);
    close(
        vol(&k().boolean(&a, &b, BoolOp::Difference).unwrap()),
        1000.0,
    );
}
