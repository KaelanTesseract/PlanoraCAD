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

// ---- Nachbesserung Runde 1 (Review Task 6) ----

fn line(x: f64, y: f64) -> Segment {
    Segment::Line {
        to: DVec2::new(x, y),
    }
}

#[test]
fn a_spike_in_a_short_knot_span_is_sampled_and_its_hole_contact_found() {
    // Grad 1, die Spitze nach (5, 2) liegt in Knotenabschnitten der Breite 0,001: bei gleichmäßiger
    // Abtastung im Parameter fiel sie früher durch (Fläche 100 statt 96, Loch nicht erkannt).
    let tol = Tolerance::default();
    let spike = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 0.501, 0.502, 0.503, 1.0, 1.0],
        vec![
            DVec3::new(10.0, 10.0, 0.0),
            DVec3::new(5.5, 10.0, 0.0),
            DVec3::new(5.0, 2.0, 0.0),
            DVec3::new(4.5, 10.0, 0.0),
            DVec3::new(0.0, 10.0, 0.0),
        ],
        vec![1.0; 5],
    )
    .unwrap();
    let outer = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            line(10.0, 0.0),
            line(10.0, 10.0),
            nurbs(spike),
            line(0.0, 0.0),
        ],
    };
    assert!(
        (outer.signed_area() - 96.0).abs() < 1e-9,
        "{}",
        outer.signed_area()
    );
    let profile = Profile {
        outer: outer.clone(),
        holes: vec![ProfileLoop::circle(DVec2::new(5.0, 5.0), 1.0)],
    };
    assert_eq!(
        extrude(&profile, &world(), 1.0, &tol).unwrap_err(),
        KernelError::ProfileLoopsIntersect
    );
    // Ohne Loch: gültiger, dichter Körper mit dem Volumen der echten Fläche.
    let solid = extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &world(),
        1.0,
        &tol,
    )
    .unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let vol = mass_properties(&mesh).volume;
    // Die Kantenpolylinien von Freiformkurven sind gleichmäßig im Parameter geteilt und dann
    // verfeinert; die Spitze wird damit nur angenähert (Fläche als ungenau gemeldet).
    assert!((vol - 96.0).abs() / 96.0 < 2e-3, "{vol}");
}

#[test]
fn a_spline_dipping_across_the_axis_between_samples_is_rejected() {
    // r(t) = 8 (t − 0,4)² − 1e-4: nur auf einem kleinen Stück um t = 0,4 negativ.
    let tol = Tolerance::default();
    let dip = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![
            DVec3::new(1.2799, 0.0, 0.0),
            DVec3::new(-1.9201, 2.0, 0.0),
            DVec3::new(2.8799, 4.0, 0.0),
        ],
        vec![1.0; 3],
    )
    .unwrap();
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![line(1.2799, 0.0), nurbs(dip), line(0.0, 4.0)],
    };
    assert_eq!(
        revolve(&chain, &world(), &tol).unwrap_err(),
        KernelError::ProfileCrossesAxis
    );
}

#[test]
fn an_inner_control_point_on_the_axis_is_refined_away() {
    // Kontrollpunkte (2,0), (0,2), (2,4): die Kurve hat r ≥ 1, der mittlere Kontrollpunkt liegt
    // aber auf der Achse (früher „innerer Pol“).
    let tol = Tolerance::default();
    let c = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(0.0, 2.0, 0.0),
            DVec3::new(2.0, 4.0, 0.0),
        ],
        vec![1.0; 3],
    )
    .unwrap();
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![line(2.0, 0.0), nurbs(c), line(0.0, 4.0)],
    };
    let solid = revolve(&chain, &world(), &tol).unwrap();
    validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = pappus(&chain);
    let vol = mass_properties(&mesh).volume;
    assert!((vol - exact).abs() / exact < 3e-3, "{vol} vs {exact}");
}

/// Ellipse um `(cx, 0)` mit Halbachsen `a`, `b`, im Uhrzeigersinn (Überstreichung −2π).
fn cw_ellipse(cx: f64, a: f64, b: f64) -> NurbsCurve {
    NurbsCurve::elliptic_arc(
        DVec3::new(cx, 0.0, 0.0),
        DVec3::X * a,
        DVec3::Y * b,
        0.0,
        -TAU,
    )
    .unwrap()
}

#[test]
fn caps_with_an_ellipse_hole_and_aligned_box_corners_are_watertight() {
    // Die Naht der Ellipse (−5, 0) und die Ecken (0, 0), (6, 0) des Rechtecklochs liegen auf
    // einer Geraden; `earcut` ließ dort früher einen T-Stoß im Deckel.
    let tol = Tolerance::default();
    let holes = || {
        vec![
            ProfileLoop::nurbs_loop(cw_ellipse(-8.0, 3.0, 2.0)).unwrap(),
            ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(6.0, 4.0)),
        ]
    };
    let rect = Profile {
        outer: ProfileLoop::rectangle(DVec2::new(-20.0, -12.0), DVec2::new(20.0, 12.0)),
        holes: holes(),
    };
    let ellipse_outer = Profile {
        outer: ProfileLoop::nurbs_loop(ellipse(20.0, 12.0)).unwrap(),
        holes: holes(),
    };
    let holes_area = PI * 6.0 + 24.0;
    for (profile, outer_area) in [(rect, 40.0 * 24.0), (ellipse_outer, PI * 240.0)] {
        let solid = extrude(&profile, &world(), 2.0, &tol).unwrap();
        validate(&solid).unwrap();
        for chord in [0.005, 0.05] {
            let mesh = tessellate(&solid, &opts(chord)).unwrap();
            assert!(is_watertight(&mesh, 1e-9), "Sehnentoleranz {chord}");
            let exact = (outer_area - holes_area) * 2.0;
            let vol = mass_properties(&mesh).volume;
            assert!((vol - exact).abs() / exact < 2e-3, "{vol} vs {exact}");
        }
    }
}

/// Geschlossene kubische Kurve mit `n` Kontrollpunkten auf einem Kreis (erster = letzter).
fn many_point_loop(r: f64, n: usize) -> NurbsCurve {
    let mut control: Vec<DVec3> = (0..n)
        .map(|i| {
            let t = TAU * i as f64 / (n - 1) as f64;
            DVec3::new(r * t.cos(), r * t.sin(), 0.0)
        })
        .collect();
    control[n - 1] = control[0];
    let inner = n - 4;
    let mut knots = vec![0.0; 4];
    knots.extend((1..=inner).map(|k| k as f64 / (inner + 1) as f64));
    knots.extend([1.0; 4]);
    NurbsCurve::new(3, knots, control, vec![1.0; n]).unwrap()
}

#[test]
fn profile_checks_with_thousands_of_control_points_stay_fast() {
    let tol = Tolerance::default();
    let profile = Profile {
        outer: ProfileLoop::nurbs_loop(many_point_loop(10.0, 4096)).unwrap(),
        holes: vec![ProfileLoop::nurbs_loop(many_point_loop(5.0, 4096)).unwrap()],
    };
    let start = std::time::Instant::now();
    let solid = extrude(&profile, &world(), 1.0, &tol).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(solid.faces.len(), 4);
    // Früher (quadratische Paarprüfung, 65 536 Abtastpunkte je Segment) 81 s im Release-Build.
    assert!(elapsed.as_secs_f64() < 5.0, "{elapsed:?}");
}

#[test]
fn a_closure_gap_within_the_tolerance_is_closed_exactly() {
    // Letzter Kontrollpunkt 5e-7 neben dem ersten: innerhalb von `tol.linear`, aber weit über der
    // Grenze, mit der die geschlossene Wandfläche gleiche Randreihen verlangt.
    let tol = Tolerance::default();
    let e = ellipse(4.0, 2.0);
    let mut control = e.control_points().to_vec();
    let n = control.len();
    control[n - 1] += DVec3::new(0.0, 5e-7, 0.0);
    let gap = NurbsCurve::new(
        e.degree(),
        e.knots().knots().to_vec(),
        control,
        e.weights().to_vec(),
    )
    .unwrap();
    let via_loop = ProfileLoop::nurbs_loop(gap.clone()).unwrap();
    let by_hand = ProfileLoop {
        start: DVec2::new(4.0, 0.0),
        segments: vec![nurbs(gap)],
    };
    for lp in [via_loop, by_hand] {
        let solid = extrude(
            &Profile {
                outer: lp,
                holes: vec![],
            },
            &world(),
            6.0,
            &tol,
        )
        .unwrap();
        validate(&solid).unwrap();
        let mesh = tessellate(&solid, &opts(0.005)).unwrap();
        assert!(is_watertight(&mesh, 1e-9));
    }
}

#[test]
fn a_self_intersecting_spline_loop_is_rejected() {
    // Der Spline von (10, 0) über (5, −4) und (5, 6) nach (0, 0) kreuzt die Grundlinie.
    let tol = Tolerance::default();
    let s = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(5.0, -4.0, 0.0),
            DVec3::new(5.0, 6.0, 0.0),
            DVec3::new(0.0, 0.0, 0.0),
        ],
        &[0.0, 1.0, 2.0, 3.0],
    )
    .unwrap();
    let lp = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![line(10.0, 0.0), nurbs(s)],
    };
    assert_eq!(
        extrude(
            &Profile {
                outer: lp,
                holes: vec![]
            },
            &world(),
            1.0,
            &tol
        )
        .unwrap_err(),
        KernelError::ProfileSelfIntersects
    );
}
