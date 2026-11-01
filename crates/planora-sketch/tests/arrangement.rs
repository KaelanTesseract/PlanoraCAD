//! Profile als ebene Anordnung wie in Fusion (Skizzen 1): jede von nicht-konstruktiven Kurven
//! umschlossene Fläche ist ein Profil.

use planora_kernel::{AnalyticKernel, Kernel, Profile};
use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::{CurveId, ExtractedProfiles};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn rect(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> (Vec<PointId>, Vec<CurveId>) {
    let p = vec![
        s.add_point(x0, y0).unwrap(),
        s.add_point(x1, y0).unwrap(),
        s.add_point(x1, y1).unwrap(),
        s.add_point(x0, y1).unwrap(),
    ];
    let l = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    (p, l)
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let a = s.add_point(a.0, a.1).unwrap();
    let b = s.add_point(b.0, b.1).unwrap();
    s.add_line(a, b).unwrap()
}

fn circle(s: &mut Sketch, x: f64, y: f64, r: f64) -> CurveId {
    let c = s.add_point(x, y).unwrap();
    s.add_circle(c, r).unwrap()
}

/// Profile der Skizze; jedes extrudiert und validiert mit dem Kernel.
fn extract(s: &Sketch) -> ExtractedProfiles {
    let e = s.profiles();
    let k = AnalyticKernel::default();
    for (i, p) in e.profiles.iter().enumerate() {
        let solid = k
            .extrude(p, s.frame(), 1.0)
            .unwrap_or_else(|err| panic!("Profil {i}: {err}"));
        k.validate(&solid)
            .unwrap_or_else(|err| panic!("Profil {i}: {err}"));
    }
    assert_eq!(e.refs.len(), e.profiles.len());
    assert_eq!(e.outlines.len(), e.profiles.len());
    assert_eq!(e.curve_loops.len(), e.profiles.len());
    e
}

/// Nettofläche (äußerer Rand minus Löcher, abgetastet).
fn area(p: &Profile) -> f64 {
    p.outer.signed_area().abs() - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>()
}

fn areas(e: &ExtractedProfiles) -> Vec<f64> {
    e.profiles.iter().map(area).collect()
}

fn assert_areas(e: &ExtractedProfiles, want: &[f64], eps: f64) {
    let got = areas(e);
    assert_eq!(got.len(), want.len(), "{got:?} statt {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < eps, "{got:?} statt {want:?}");
    }
}

#[test]
fn a_diagonal_splits_a_rectangle_into_two_triangles() {
    let mut s = sketch();
    let (p, l) = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let diag = s.add_line(p[0], p[2]).unwrap();
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[50.0, 50.0], 1e-9);
    // unteres Dreieck zuerst (enthält die Kurve mit der kleinsten Id)
    assert_eq!(e.curve_loops[0], vec![l[0], l[1], diag]);
    assert_eq!(e.curve_loops[1], vec![l[2], l[3], diag]);
    let lower = &e.refs[0];
    assert_eq!(lower.outer.len(), 3);
    assert!(lower.holes.is_empty());
    for r in &lower.outer {
        // Rechteckkanten laufen gegen den Uhrzeigersinn: Fläche links; die Diagonale läuft
        // von (0,0) nach (10,10), das untere Dreieck liegt rechts davon.
        assert_eq!(r.left, r.curve != diag, "{r:?}");
        assert!((r.u0, r.u1) == (0.0, 1.0), "{r:?}");
    }
    let upper = &e.refs[1];
    assert_eq!(upper.outer.len(), 3);
    assert!(upper.outer.iter().all(|r| r.left), "{upper:?}");
    let d = upper.outer.iter().find(|r| r.curve == diag).unwrap();
    assert!((d.len - 200f64.sqrt()).abs() < 1e-12);
}

#[test]
fn crossing_rectangles_give_three_faces() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    rect(&mut s, 5.0, 5.0, 15.0, 15.0);
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    let mut a = areas(&e);
    a.sort_by(f64::total_cmp);
    assert_eq!(a.len(), 3);
    for (g, w) in a.iter().zip([25.0, 75.0, 75.0]) {
        assert!((g - w).abs() < 1e-9, "{a:?}");
    }
}

#[test]
fn overlapping_circles_give_three_faces() {
    let mut s = sketch();
    circle(&mut s, 0.0, 0.0, 5.0);
    circle(&mut s, 6.0, 0.0, 5.0);
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 3);
    // Linse: 2·(r²·acos(d/2r) − d/2·√(r² − d²/4))
    let lens = 2.0 * (25.0 * (0.6f64).acos() - 3.0 * 4.0);
    let a = areas(&e);
    assert!(a.iter().any(|x| (x - lens).abs() < 0.1), "{a:?} / {lens}");
    let total: f64 = a.iter().sum();
    assert!((total - (2.0 * PI * 25.0 - lens)).abs() < 0.3, "{total}");
}

#[test]
fn a_circle_in_a_rectangle_gives_the_ring_and_the_disk() {
    let mut s = sketch();
    let (_, l) = rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let c = circle(&mut s, 10.0, 10.0, 4.0);
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 2);
    assert_eq!(
        e.profiles[0].holes.len(),
        1,
        "Ring zuerst (kleinste Kurven-Id)"
    );
    assert!(e.profiles[1].holes.is_empty());
    assert_eq!(e.curve_loops, vec![l.clone(), vec![c]]);
    assert_areas(&e, &[400.0 - PI * 16.0, PI * 16.0], 0.1);
    // Loch: Kreis gegen seine Richtung, Fläche (Ring) rechts davon
    let hole = &e.refs[0].holes;
    assert_eq!(hole.len(), 1);
    assert_eq!((hole[0].curve, hole[0].left), (c, false));
    assert!((hole[0].u0, hole[0].u1) == (0.0, 1.0), "{hole:?}");
    assert_eq!(e.refs[1].outer.len(), 1);
    assert!(e.refs[1].outer[0].left);
    assert!((e.refs[1].outer[0].len - 8.0 * PI).abs() < 1e-9);
    // Umrisse: äußerer zuerst, dann das Loch
    assert_eq!(e.outlines[0].len(), 2);
    assert_eq!(e.outlines[1].len(), 1);
    assert_eq!(e.profile_at(DVec2::new(2.0, 2.0)), Some(0), "Ring");
    assert_eq!(e.profile_at(DVec2::new(10.0, 11.0)), Some(1), "Kreisfläche");
    assert_eq!(e.profile_at(DVec2::new(30.0, 0.0)), None);
}

#[test]
fn a_line_through_a_circle_gives_two_half_disks() {
    let mut s = sketch();
    circle(&mut s, 0.0, 0.0, 5.0);
    line(&mut s, (-8.0, 0.0), (8.0, 0.0));
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[PI * 12.5, PI * 12.5], 0.1);
    // Viertelkreis-Umrechnung: oberer Halbkreis von u = 0 bis 0,5
    let upper = e.profile_at(DVec2::new(0.0, 2.0)).unwrap();
    let arc = e.refs[upper]
        .outer
        .iter()
        .find(|r| r.u1 - r.u0 > 0.4)
        .unwrap();
    assert!(
        (arc.u0 - 0.0).abs() < 1e-12 && (arc.u1 - 0.5).abs() < 1e-12,
        "{arc:?}"
    );
    let lower = e.profile_at(DVec2::new(0.0, -2.0)).unwrap();
    let arc = e.refs[lower]
        .outer
        .iter()
        .find(|r| r.u1 - r.u0 > 0.4)
        .unwrap();
    assert!(
        (arc.u0 - 0.5).abs() < 1e-12 && (arc.u1 - 1.0).abs() < 1e-12,
        "{arc:?}"
    );
}

#[test]
fn a_stub_at_a_corner_is_ignored() {
    let mut s = sketch();
    let (p, _) = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let extra = s.add_point(9.0, 9.0).unwrap();
    s.add_line(p[2], extra).unwrap();
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[16.0], 1e-9);
}

#[test]
fn a_construction_line_across_does_not_split() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let l = line(&mut s, (-1.0, 2.0), (5.0, 2.0));
    s.set_construction(l, true).unwrap();
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[16.0], 1e-9);
}

#[test]
fn triangles_touching_in_a_corner_are_two_profiles() {
    let mut s = sketch();
    let o = s.add_point(0.0, 0.0).unwrap();
    let pts: Vec<PointId> = [(4.0, 0.0), (4.0, 4.0), (-4.0, 0.0), (-4.0, -4.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    s.add_line(o, pts[0]).unwrap();
    s.add_line(pts[0], pts[1]).unwrap();
    s.add_line(pts[1], o).unwrap();
    s.add_line(o, pts[2]).unwrap();
    s.add_line(pts[2], pts[3]).unwrap();
    s.add_line(pts[3], o).unwrap();
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[8.0, 8.0], 1e-9);
}

#[test]
fn a_figure_eight_spline_gives_two_profiles() {
    let mut s = sketch();
    let p: Vec<PointId> = [
        (0.0, 0.0),
        (2.0, 1.5),
        (4.0, 0.3),
        (6.0, -1.5),
        (8.0, 0.0),
        (6.0, 1.5),
        (4.0, -0.3),
        (2.0, -1.5),
        (0.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| s.add_point(x, y).unwrap())
    .collect();
    s.add_spline(&p).unwrap();
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 2);
    let a = areas(&e);
    assert!(a.iter().all(|&x| x > 3.0), "{a:?}");
}

#[test]
fn collinear_shared_edges_are_merged() {
    // zwei Rechtecke nebeneinander, jedes mit eigener Kante bei x = 10
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    rect(&mut s, 10.0, 0.0, 20.0, 10.0);
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[100.0, 100.0], 1e-9);

    // nur teilweise überlappende Kante
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    rect(&mut s, 10.0, 2.0, 20.0, 8.0);
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[100.0, 60.0], 1e-9);
}

#[test]
fn two_equal_circles_are_one_profile() {
    let mut s = sketch();
    let a = circle(&mut s, 1.0, 2.0, 3.0);
    circle(&mut s, 1.0, 2.0, 3.0);
    let e = extract(&s);
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_areas(&e, &[PI * 9.0], 0.05);
    assert_eq!(e.curve_loops, vec![vec![a]], "kleinere Kurven-Id bleibt");
}

#[test]
fn coincident_curves_outside_the_special_cases_give_no_slivers() {
    // Ellipse mit gleichen Halbachsen auf einem Kreis (gedrehte Achse)
    let mut s = sketch();
    circle(&mut s, 0.0, 0.0, 5.0);
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_ellipse(c, DVec2::new(3.0, 4.0), 5.0).unwrap();
    let e = extract(&s);
    let a = areas(&e);
    assert!(a.iter().all(|&x| x > 1.0), "{a:?}");
    assert_eq!(e.profiles.len(), 1, "{a:?} / {:?}", e.rejected);

    // zwei gleiche Splines, mit einer Linie geschlossen
    let mut s = sketch();
    let p: Vec<PointId> = [(0.0, 0.0), (3.0, 4.0), (7.0, 3.0), (10.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    s.add_spline(&p).unwrap();
    s.add_spline(&p).unwrap();
    s.add_line(p[3], p[0]).unwrap();
    let e = extract(&s);
    let a = areas(&e);
    assert!(a.iter().all(|&x| x > 1.0), "{a:?}");
    assert_eq!(e.profiles.len(), 1, "{a:?} / {:?}", e.rejected);
}

#[test]
fn the_legacy_extraction_keeps_the_old_rules() {
    // Kreis im Rechteck: früher ein Profil mit Loch; kreuzende Rechtecke: abgelehnt.
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    circle(&mut s, 10.0, 10.0, 4.0);
    let e = s.legacy_profiles();
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.profiles[0].holes.len(), 1);
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    rect(&mut s, 5.0, 5.0, 15.0, 15.0);
    let e = s.legacy_profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected.len(), 2);
}
