use planora_kernel::{AnalyticKernel, Kernel, Segment};
use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn rect(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<PointId> {
    let p = vec![
        s.add_point(x0, y0).unwrap(),
        s.add_point(x1, y0).unwrap(),
        s.add_point(x1, y1).unwrap(),
        s.add_point(x0, y1).unwrap(),
    ];
    for i in 0..4 {
        s.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    p
}

#[test]
fn a_rectangle_is_one_profile() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 4.0);
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.curve_loops.len(), 1);
    assert_eq!(e.curve_loops[0].len(), 4);
    let p = &e.profiles[0];
    assert!(p.holes.is_empty());
    assert_eq!(p.outer.segments.len(), 4);
    assert!((p.outer.signed_area().abs() - 12.0).abs() < 1e-9);
    assert!(p.outer.is_closed(s.tolerance()));
}

#[test]
fn a_circle_inside_a_rectangle_becomes_a_hole() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let c = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(c, 4.0).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.profiles[0].holes.len(), 1);
    // das Profil lässt sich wirklich extrudieren
    let solid = AnalyticKernel::default()
        .extrude(&e.profiles[0], s.frame(), 2.0)
        .unwrap();
    AnalyticKernel::default().validate(&solid).unwrap();
}

#[test]
fn nesting_alternates_outer_and_hole() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(c, 10.0).unwrap();
    s.add_circle(c, 6.0).unwrap();
    s.add_circle(c, 2.0).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 2, "Ring plus innere Insel");
    let ring = e.profiles.iter().find(|p| !p.holes.is_empty()).unwrap();
    assert_eq!(ring.holes.len(), 1);
    let island = e.profiles.iter().find(|p| p.holes.is_empty()).unwrap();
    assert!(
        (island.outer.signed_area().abs() - PI * 4.0).abs() < 0.1,
        "abgetastete Fläche"
    );
}

#[test]
fn two_separate_rectangles_are_two_profiles() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    rect(&mut s, 10.0, 0.0, 13.0, 3.0);
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    assert!(e.profiles.iter().all(|p| p.holes.is_empty()));
}

#[test]
fn construction_geometry_is_ignored() {
    let mut s = sketch();
    let p = rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    let diag = s.add_line(p[0], p[2]).unwrap();
    s.set_construction(diag, true).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
}

#[test]
fn open_chains_and_t_junctions_are_rejected_with_a_reason() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let c = s.add_point(5.0, 5.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected.len(), 1);
    assert_eq!(e.rejected[0].reason, SketchError::OpenLoop);

    let mut s = sketch();
    let p = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let extra = s.add_point(9.0, 9.0).unwrap();
    s.add_line(p[0], extra).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert!(
        matches!(e.rejected[0].reason, SketchError::AmbiguousTopology(_)),
        "{:?}",
        e.rejected
    );
}

#[test]
fn bow_ties_and_crossing_loops_are_rejected() {
    let mut s = sketch();
    let p = [
        s.add_point(0.0, 0.0).unwrap(),
        s.add_point(4.0, 4.0).unwrap(),
        s.add_point(4.0, 0.0).unwrap(),
        s.add_point(0.0, 4.0).unwrap(),
    ];
    for i in 0..4 {
        s.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected[0].reason, SketchError::SelfIntersecting);

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 6.0, 6.0);
    rect(&mut s, 3.0, 3.0, 9.0, 9.0);
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected.len(), 2);
    assert!(
        e.rejected
            .iter()
            .all(|r| r.reason == SketchError::SelfIntersecting)
    );
}

#[test]
fn arcs_follow_the_traversal_direction() {
    // Halbscheibe: Linie von (-1,0) nach (1,0) und Bogen (gegen den Uhrzeigersinn) von (1,0)
    // über (0,1) nach (-1,0)
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let l = s.add_point(-1.0, 0.0).unwrap();
    let r = s.add_point(1.0, 0.0).unwrap();
    s.add_line(l, r).unwrap();
    s.add_arc(c, r, l).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    let outer = &e.profiles[0].outer;
    // `signed_area` tastet mit 64 Schritten je Vollkreis ab: Der Halbkreis wird zum 32-Eck mit
    // Fläche 16·sin(π/32) ≈ 1,56827, also 2,5e-3 unter π/2 (Plan sah 1e-3 vor, unerreichbar).
    assert!(
        (outer.signed_area().abs() - PI / 2.0).abs() < 5e-3,
        "{}",
        outer.signed_area()
    );
    assert!(outer.is_closed(s.tolerance()));
    let arcs: Vec<f64> = outer
        .segments
        .iter()
        .filter_map(|sg| match sg {
            Segment::Arc { sweep, .. } => Some(*sweep),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 1);
    assert!((arcs[0].abs() - PI).abs() < 1e-9);
    let solid = AnalyticKernel::default()
        .extrude(&e.profiles[0], s.frame(), 1.0)
        .unwrap();
    AnalyticKernel::default().validate(&solid).unwrap();
}

#[test]
fn nearly_touching_endpoints_are_the_same_node() {
    // zwei Linien, deren Endpunkte 1e-8 auseinander liegen, schließen die Schleife trotzdem
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    let c = s.add_point(4.0, 4.0).unwrap();
    let c2 = s.add_point(4.0 + 1e-8, 4.0).unwrap();
    let d = s.add_point(0.0, 4.0).unwrap();
    let d2 = s.add_point(1e-8, 4.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    s.add_line(c2, d).unwrap();
    s.add_line(d2, a).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    let _ = DVec2::ZERO;
}

// Zusätzliche Tests (nicht aus dem Plan).

#[test]
fn a_line_back_and_forth_encloses_no_area() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, a).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected.len(), 1);
    assert!(
        matches!(e.rejected[0].reason, SketchError::Degenerate(_)),
        "{:?}",
        e.rejected
    );
}

#[test]
fn an_arc_traversed_backwards_gets_a_negative_sweep() {
    // Viertelkreis: Bogen (0,0)-Mitte von (2,0) nach (0,2); die erste Kurve (Linie) läuft
    // (2,0) -> (0,0), dann (0,0) -> (0,2), der Bogen wird also rückwärts durchlaufen.
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(2.0, 0.0).unwrap();
    let b = s.add_point(0.0, 2.0).unwrap();
    s.add_line(a, c).unwrap();
    s.add_line(c, b).unwrap();
    s.add_arc(c, a, b).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    let outer = &e.profiles[0].outer;
    assert!(outer.is_closed(s.tolerance()));
    let sweep = outer
        .segments
        .iter()
        .find_map(|sg| match sg {
            Segment::Arc { sweep, .. } => Some(*sweep),
            _ => None,
        })
        .unwrap();
    assert!((sweep + PI / 2.0).abs() < 1e-9, "{sweep}");
    let solid = AnalyticKernel::default()
        .extrude(&e.profiles[0], s.frame(), 1.0)
        .unwrap();
    AnalyticKernel::default().validate(&solid).unwrap();
}

#[test]
fn two_holes_and_an_island_extrude() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 30.0, 20.0);
    rect(&mut s, 2.0, 2.0, 12.0, 18.0);
    let c = s.add_point(22.0, 10.0).unwrap();
    s.add_circle(c, 5.0).unwrap();
    rect(&mut s, 4.0, 4.0, 8.0, 8.0);
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 2);
    assert_eq!(e.profiles[0].holes.len(), 2);
    assert_eq!(e.curve_loops[0].len(), 4);
    for p in &e.profiles {
        let solid = AnalyticKernel::default()
            .extrude(p, s.frame(), 1.0)
            .unwrap();
        AnalyticKernel::default().validate(&solid).unwrap();
    }
}
