use planora_kernel::{Profile, ProfileLoop, Segment, extrude, revolve, validate};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

fn world() -> Frame {
    Frame::from_origin_normal(DVec3::ZERO, DVec3::Z)
}
fn opts(c: f64) -> TessellationOptions {
    TessellationOptions { chord_tolerance: c }
}

fn ellipse(a: f64, b: f64) -> NurbsCurve {
    NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, b, 0.0),
        0.0,
        TAU,
    )
    .unwrap()
}

#[test]
fn an_extruded_ellipse_has_the_exact_volume() {
    let tol = Tolerance::default();
    let profile = Profile {
        outer: ProfileLoop::nurbs_loop(ellipse(4.0, 2.0)).unwrap(),
        holes: vec![],
    };
    let solid = extrude(&profile, &world(), 6.0, &tol).unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    let exact = PI * 4.0 * 2.0 * 6.0;
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 2e-3, "{vol}");
    // Rollen: Boden, Deckel, Wand 1
    let roles: Vec<String> = solid
        .faces
        .iter()
        .filter_map(|(_, f)| f.tag.as_ref().map(|t| t.role.clone()))
        .collect();
    for r in ["Boden", "Deckel", "Wand 1"] {
        assert!(roles.iter().any(|x| x == r), "{r} fehlt: {roles:?}");
    }
}

#[test]
fn a_profile_with_a_spline_edge_and_lines_extrudes_to_a_valid_solid() {
    let tol = Tolerance::default();
    // Unten eine Gerade von (0,0) nach (10,0), rechts hoch, oben ein Spline zurück, links runter
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 6.0, 0.0),
            DVec3::new(7.0, 8.0, 0.0),
            DVec3::new(3.0, 5.0, 0.0),
            DVec3::new(0.0, 7.0, 0.0),
        ],
        &[0.0, 1.0, 2.0, 3.0],
    )
    .unwrap();
    let lp = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(10.0, 6.0),
            },
            Segment::Nurbs {
                curve: std::sync::Arc::new(spline),
            },
            Segment::Line {
                to: DVec2::new(0.0, 0.0),
            },
        ],
    };
    let profile = Profile {
        outer: lp,
        holes: vec![],
    };
    let solid = extrude(&profile, &world(), 3.0, &tol).unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    let vol = mass_properties(&mesh).volume;
    // Volumen = Fläche des Profils (aus der Abtastung) · Höhe
    let area = profile.outer.signed_area().abs();
    assert!(
        (vol - area * 3.0).abs() / (area * 3.0) < 2e-3,
        "{vol} vs {}",
        area * 3.0
    );
}

#[test]
fn a_revolved_ellipse_half_is_an_ellipsoid() {
    // Halbe Ellipse (a entlang r, b entlang z) um die z-Achse: Ellipsoid, V = 4/3 π a² b
    let tol = Tolerance::default();
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 3.0, 0.0),
        DVec3::new(2.0, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap(); // (r,z)-Ebene: x=r, y=z
    // Kette: Halbellipse von (0,3) über (2,0) nach (0,-3); sie beginnt und endet auf der Achse (r = 0)
    let chain = ProfileLoop {
        start: DVec2::new(0.0, 3.0),
        segments: vec![Segment::Nurbs {
            curve: std::sync::Arc::new(half),
        }],
    };
    let solid = revolve(&chain, &world(), &tol).unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    let exact = 4.0 / 3.0 * PI * 2.0 * 2.0 * 3.0;
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 3e-3, "{vol}");
}

#[test]
fn bad_nurbs_segments_are_rejected_without_panic() {
    let tol = Tolerance::default();
    // Kurve springt nicht an den Start an
    let off =
        NurbsCurve::elliptic_arc(DVec3::new(50.0, 0.0, 0.0), DVec3::X, DVec3::Y, 0.0, 1.0).unwrap();
    let lp = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Nurbs {
                curve: std::sync::Arc::new(off),
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    assert!(
        extrude(
            &Profile {
                outer: lp,
                holes: vec![]
            },
            &world(),
            1.0,
            &tol
        )
        .is_err()
    );
    // Kurve mit z != 0
    let tilted =
        NurbsCurve::elliptic_arc(DVec3::new(0.0, 0.0, 2.0), DVec3::X, DVec3::Y, 0.0, TAU).unwrap();
    assert!(ProfileLoop::nurbs_loop(tilted).is_err());
}

// ---- Zusätzliche Tests (über den Plan hinaus) ----

use planora_kernel::KernelError;
use planora_mesh::is_watertight;
use std::sync::Arc;

fn nurbs(c: NurbsCurve) -> Segment {
    Segment::Nurbs { curve: Arc::new(c) }
}

fn roles(solid: &planora_kernel::Solid) -> Vec<String> {
    solid
        .faces
        .iter()
        .filter_map(|(_, f)| f.tag.as_ref().map(|t| t.role.clone()))
        .collect()
}

/// Volumen eines Drehkörpers aus dem geschlossen gedachten (r, z)-Linienzug (Guldin: 2π · A · r̄).
fn pappus(chain: &ProfileLoop) -> f64 {
    let p = chain.sample(4096);
    let n = p.len();
    // Erstes Flächenmoment ∫ r dA über den geschlossenen Linienzug.
    let moment: f64 = (0..n)
        .map(|i| {
            let (s, t) = (p[i], p[(i + 1) % n]);
            (s.x + t.x) * s.perp_dot(t) / 6.0
        })
        .sum();
    (TAU * moment).abs()
}

#[test]
fn a_chain_with_a_spline_revolves_to_the_pappus_volume() {
    let tol = Tolerance::default();
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(4.0, 1.5, 0.0),
            DVec3::new(3.0, 3.5, 0.0),
            DVec3::new(2.0, 5.0, 0.0),
        ],
        &[0.0, 1.0, 2.0, 3.0],
    )
    .unwrap();
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(3.0, 0.0),
            },
            nurbs(spline),
            Segment::Line {
                to: DVec2::new(0.0, 5.0),
            },
        ],
    };
    let solid = revolve(&chain, &world(), &tol).unwrap();
    validate(&solid).unwrap();
    let r = roles(&solid);
    for name in ["Fläche 1", "Fläche 2", "Fläche 3"] {
        assert!(r.iter().any(|x| x == name), "{name} fehlt: {r:?}");
    }
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = pappus(&chain);
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 3e-3, "{vol} vs {exact}");
}

#[test]
fn a_revolved_ellipse_half_running_upwards_is_the_same_ellipsoid() {
    // Gegenrichtung zum Plantest: von (0,-3) über (2,0) nach (0,3); die natürliche Normale der
    // Drehfläche zeigt dann anders herum, das Volumen muss trotzdem positiv und gleich sein.
    let tol = Tolerance::default();
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, -3.0, 0.0),
        DVec3::new(2.0, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -3.0),
        segments: vec![nurbs(half)],
    };
    let solid = revolve(&chain, &world(), &tol).unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = 4.0 / 3.0 * PI * 2.0 * 2.0 * 3.0;
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 3e-3, "{vol}");
}

#[test]
fn an_ellipse_hole_and_a_clockwise_ellipse_are_oriented_automatically() {
    let tol = Tolerance::default();
    // Loch gegen den Uhrzeigersinn eingegeben: wird intern umgedreht.
    let hole = NurbsCurve::elliptic_arc(
        DVec3::new(10.0, 10.0, 0.0),
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(0.0, 2.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::nurbs_loop(hole).unwrap()],
    };
    let solid = extrude(&profile, &world(), 2.0, &tol).unwrap();
    validate(&solid).unwrap();
    assert!(roles(&solid).iter().any(|x| x == "Loch 1, Wand 1"));
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = (400.0 - PI * 8.0) * 2.0;
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 2e-3, "{vol}");

    // Äußere Ellipse im Uhrzeigersinn, in einem gekippten Koordinatensystem.
    let cw =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 4.0, DVec3::Y * 2.0, 0.0, -TAU).unwrap();
    let frame = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 1.0));
    let profile = Profile {
        outer: ProfileLoop::nurbs_loop(cw).unwrap(),
        holes: vec![],
    };
    let solid = extrude(&profile, &frame, 6.0, &tol).unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = PI * 4.0 * 2.0 * 6.0;
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 2e-3, "{vol}");
}

#[test]
fn bad_nurbs_profiles_and_chains_are_named_errors() {
    let tol = Tolerance::default();
    // Ellipsenloch berührt die äußere Schleife (x = 0 und x = 10).
    let touching = NurbsCurve::elliptic_arc(
        DVec3::new(5.0, 5.0, 0.0),
        DVec3::X * 5.0,
        DVec3::Y * 2.0,
        0.0,
        TAU,
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)),
        holes: vec![ProfileLoop::nurbs_loop(touching).unwrap()],
    };
    assert_eq!(
        extrude(&profile, &world(), 1.0, &tol).unwrap_err(),
        KernelError::ProfileLoopsIntersect
    );
    // Eine geschlossene Kurve als eines von mehreren Segmenten.
    let closed = ellipse(1.0, 1.0);
    let lp = ProfileLoop {
        start: DVec2::new(1.0, 0.0),
        segments: vec![
            nurbs(closed),
            Segment::Line {
                to: DVec2::new(3.0, 3.0),
            },
            Segment::Line {
                to: DVec2::new(1.0, 0.0),
            },
        ],
    };
    assert!(
        extrude(
            &Profile {
                outer: lp,
                holes: vec![]
            },
            &world(),
            1.0,
            &tol
        )
        .is_err()
    );
    // Spline, die im Inneren über die Achse läuft.
    let crossing = NurbsCurve::natural_cubic(
        &[
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(-1.0, 2.0, 0.0),
            DVec3::new(2.0, 4.0, 0.0),
        ],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(3.0, 0.0),
            },
            nurbs(crossing),
            Segment::Line {
                to: DVec2::new(0.0, 4.0),
            },
        ],
    };
    assert_eq!(
        revolve(&chain, &world(), &tol).unwrap_err(),
        KernelError::ProfileCrossesAxis
    );
    // Spline, die die Achse im Inneren nur berührt (r = 2·(1 − 2t)², also r = 0 bei t = 0,5, nie negativ).
    let touching_axis = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(-2.0, 2.0, 0.0),
            DVec3::new(2.0, 4.0, 0.0),
        ],
        vec![1.0; 3],
    )
    .unwrap();
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            nurbs(touching_axis),
            Segment::Line {
                to: DVec2::new(0.0, 4.0),
            },
        ],
    };
    assert_eq!(
        revolve(&chain, &world(), &tol).unwrap_err(),
        KernelError::ProfileCrossesAxis
    );
}
