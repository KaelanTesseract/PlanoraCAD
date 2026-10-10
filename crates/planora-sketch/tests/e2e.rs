use planora_kernel::{AnalyticKernel, BoolOp, Kernel};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::SketchError;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::Sketch;
use planora_sketch::solid::{extrude_profile, revolve_profile};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Sehnenfehler für Volumenmessungen. Mit der Vorgabe (0,01 mm) fehlen dem Zylinder mit
/// r = 3 durch das einbeschriebene 39-Eck ≈ 0,43 % (0,61 mm³ > 0,5); 0,001 mm drückt den
/// Fehler der Tessellierung unter 0,05 %, die Schranken der Tests bleiben unverändert.
const VOLUME_CHORD_TOLERANCE: f64 = 1e-3;

fn volume(solid: &planora_kernel::Solid) -> f64 {
    let options = TessellationOptions {
        chord_tolerance: VOLUME_CHORD_TOLERANCE,
    };
    let mesh = tessellate(solid, &options).unwrap();
    mass_properties(&mesh).volume
}

#[test]
fn a_dimensioned_rectangle_extrudes_to_the_right_volume() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let r = s
        .add_rectangle(DVec2::new(1.0, 1.0), DVec2::new(30.0, 20.0))
        .unwrap();
    s.add_constraint(K::Fixed(r.points[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(r.points[0], r.points[1], 40.0))
        .unwrap();
    s.add_constraint(K::Distance(r.points[1], r.points[2], 25.0))
        .unwrap();
    assert_eq!(s.solve().dof, 0);
    let e = s.profiles();
    let solid = extrude_profile(&k(), &s, &e.profiles[0], 7.0).unwrap();
    k().validate(&solid).unwrap();
    assert!((volume(&solid) - 40.0 * 25.0 * 7.0).abs() < 1e-3);
}

#[test]
fn a_sketch_on_another_plane_extrudes_along_its_normal() {
    let frame = Frame::from_origin_normal(DVec3::new(0.0, 5.0, 0.0), DVec3::Y);
    let mut s = Sketch::new(frame, "S", Tolerance::default());
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(c, 2.0).unwrap();
    let solid = extrude_profile(&k(), &s, &s.profiles().profiles[0], 3.0).unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    let ys: Vec<f64> = mesh.positions.iter().map(|p| p.y).collect();
    let (lo, hi) = (
        ys.iter().cloned().fold(f64::MAX, f64::min),
        ys.iter().cloned().fold(f64::MIN, f64::max),
    );
    assert!((hi - lo - 3.0).abs() < 1e-4, "Höhe entlang der Normalen");
    assert!(
        (lo - 5.0).abs() < 1e-4 || (hi - 5.0).abs() < 1e-4,
        "Sockel liegt in der Skizzenebene"
    );
}

#[test]
fn revolving_a_rectangle_next_to_an_axis_gives_a_cylinder() {
    for side in [1.0, -1.0] {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let a = s.add_point(0.0, -1.0).unwrap();
        let b = s.add_point(0.0, 6.0).unwrap();
        let axis = s.add_line(a, b).unwrap();
        s.set_construction(axis, true).unwrap();
        s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(side * 3.0, 5.0))
            .unwrap();
        let e = s.profiles();
        assert_eq!(e.profiles.len(), 1, "{:?}", e.rejected);
        let solid = revolve_profile(&k(), &s, &e.profiles[0], axis).unwrap();
        k().validate(&solid).unwrap();
        assert!(
            (volume(&solid) - PI * 9.0 * 5.0).abs() < 0.5,
            "Seite {side}"
        );
    }
}

#[test]
fn revolving_a_half_disc_gives_a_sphere() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, -4.0).unwrap();
    let b = s.add_point(0.0, 4.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    let c = s.add_point(0.0, 0.0).unwrap();
    let top = s.add_point(0.0, 4.0).unwrap();
    let bottom = s.add_point(0.0, -4.0).unwrap();
    s.add_line(top, bottom).unwrap();
    s.add_arc(c, bottom, top).unwrap(); // gegen den Uhrzeigersinn von unten über rechts nach oben
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1, "{:?}", e.rejected);
    let solid = revolve_profile(&k(), &s, &e.profiles[0], axis).unwrap();
    k().validate(&solid).unwrap();
    assert!((volume(&solid) - 4.0 / 3.0 * PI * 64.0).abs() / (4.0 / 3.0 * PI * 64.0) < 0.01);
}

#[test]
fn bad_revolve_inputs_are_rejected_with_a_reason() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(-5.0, 0.0).unwrap();
    let b = s.add_point(-5.0, 6.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(3.0, 5.0))
        .unwrap();
    let p = &s.profiles().profiles[0];
    assert!(
        matches!(
            revolve_profile(&k(), &s, p, axis),
            Err(SketchError::Degenerate(_))
        ),
        "keine Kante auf der Achse"
    );

    // Achse durch das Profil: liegt auf beiden Seiten
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(1.0, -1.0).unwrap();
    let b = s.add_point(1.0, 6.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(3.0, 5.0))
        .unwrap();
    let p = &s.profiles().profiles[0];
    assert!(revolve_profile(&k(), &s, p, axis).is_err());

    // Profile mit Loch
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, -1.0).unwrap();
    let b = s.add_point(0.0, 20.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0))
        .unwrap();
    let c = s.add_point(5.0, 5.0).unwrap();
    s.add_circle(c, 1.0).unwrap();
    let p = &s.profiles().profiles[0];
    assert!(matches!(
        revolve_profile(&k(), &s, p, axis),
        Err(SketchError::UnsupportedForSolid(_))
    ));
}

#[test]
fn sketch_solids_combine_with_boolean_operations() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    s.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0))
        .unwrap();
    let c = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(c, 4.0).unwrap();
    let plate = extrude_profile(&k(), &s, &s.profiles().profiles[0], 5.0).unwrap();
    let mut t = Sketch::new(Frame::WORLD, "T", Tolerance::default());
    t.add_rectangle(DVec2::new(5.0, 5.0), DVec2::new(8.0, 8.0))
        .unwrap();
    let pin = extrude_profile(&k(), &t, &t.profiles().profiles[0], 9.0).unwrap();
    let both = k().boolean(&plate, &pin, BoolOp::Union).unwrap();
    k().validate(&both).unwrap();
    assert!(volume(&both) > volume(&plate));
}

#[test]
fn a_profile_on_both_sides_of_the_axis_is_named_as_such() {
    // Kante (0,0)–(0,2) liegt auf der Achse, der Punkt (−1, 3) links davon, der Rest rechts.
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, -1.0).unwrap();
    let b = s.add_point(0.0, 6.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    let corners = [(0.0, 0.0), (0.0, 2.0), (-1.0, 3.0), (2.0, 3.0), (2.0, 0.0)];
    let pts: Vec<_> = corners
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    for i in 0..pts.len() {
        s.add_line(pts[i], pts[(i + 1) % pts.len()]).unwrap();
    }
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1, "{:?}", e.rejected);
    match revolve_profile(&k(), &s, &e.profiles[0], axis) {
        Err(SketchError::Degenerate(text)) => assert!(text.contains("beiden Seiten"), "{text}"),
        other => panic!("erwartet: beide Seiten, erhalten: {other:?}"),
    }
}
