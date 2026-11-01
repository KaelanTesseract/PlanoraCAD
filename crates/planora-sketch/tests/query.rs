use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn polylines_hit_the_exact_end_points() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(2.0, 0.0).unwrap();
    let b = s.add_point(0.0, 2.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    let pl = s.curve_polyline(arc, 64).unwrap();
    assert_eq!(*pl.first().unwrap(), DVec2::new(2.0, 0.0));
    assert!((*pl.last().unwrap() - DVec2::new(0.0, 2.0)).length() < 1e-12);
    assert!(pl.iter().all(|p| (p.length() - 2.0).abs() < 1e-9));
    assert!(pl.len() >= 16);
    let circle = s.add_circle(c, 1.0).unwrap();
    let cl = s.curve_polyline(circle, 32).unwrap();
    assert!(
        (cl[0] - *cl.last().unwrap()).length() < 1e-12,
        "Kreis ist geschlossen"
    );
    let line = s.add_line(a, b).unwrap();
    assert_eq!(s.curve_polyline(line, 64).unwrap().len(), 2);
    assert!(
        s.curve_polyline(planora_kernel::Id::from_index(77), 8)
            .is_none()
    );
}

#[test]
fn picking_prefers_points_and_respects_the_radius() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert_eq!(s.pick_point(DVec2::new(0.3, 0.2), 0.5), Some(a));
    assert_eq!(s.pick_point(DVec2::new(5.0, 0.0), 0.5), None);
    assert_eq!(s.pick_curve(DVec2::new(5.0, 0.4), 0.5), Some(l));
    assert_eq!(s.pick_curve(DVec2::new(5.0, 2.0), 0.5), None);
    assert!((s.distance_to_curve(l, DVec2::new(5.0, 3.0)).unwrap() - 3.0).abs() < 1e-12);
    let (lo, hi) = s.bounds().unwrap();
    assert_eq!((lo, hi), (DVec2::ZERO, DVec2::new(10.0, 0.0)));
    assert!(sketch().bounds().is_none());
}

#[test]
fn extent_includes_curves() {
    let mut s = sketch();
    let c = s.add_point(1.0, 2.0).unwrap();
    s.add_circle(c, 5.0).unwrap();
    assert_eq!(
        s.bounds().unwrap(),
        (DVec2::new(1.0, 2.0), DVec2::new(1.0, 2.0))
    );
    let (lo, hi) = s.extent().unwrap();
    assert!((lo - DVec2::new(-4.0, -3.0)).length() < 1e-9, "{lo:?}");
    assert!((hi - DVec2::new(6.0, 7.0)).length() < 1e-9, "{hi:?}");
    assert!(sketch().extent().is_none());
}

// ---- Kette (Skizzen 2, Task 7) --------------------------------------------------------------

fn free_line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = s.add_point(a.0, a.1).unwrap();
    let q = s.add_point(b.0, b.1).unwrap();
    s.add_line(p, q).unwrap()
}

fn line_ends(s: &Sketch, c: CurveId) -> (PointId, PointId) {
    match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!("keine Linie"),
    }
}

fn sorted(mut v: Vec<CurveId>) -> Vec<CurveId> {
    v.sort();
    v
}

#[test]
fn a_closed_rectangle_is_one_chain() {
    let mut s = sketch();
    let rect = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(4.0, 2.0))
        .unwrap();
    let alone = free_line(&mut s, (9.0, 9.0), (12.0, 9.0));
    assert_eq!(
        s.connected_chain(rect.lines[2]),
        sorted(rect.lines.to_vec())
    );
    assert_eq!(s.connected_chain(alone), vec![alone]);
}

#[test]
fn coincident_end_points_connect_also_over_a_loose_point() {
    let mut s = sketch();
    let l1 = free_line(&mut s, (0.0, 0.0), (1.0, 0.0));
    let l2 = free_line(&mut s, (1.0, 0.0), (2.0, 0.0));
    let (_, l1_end) = line_ends(&s, l1);
    let (l2_start, l2_end) = line_ends(&s, l2);
    s.add_constraint(ConstraintKind::Coincident(l1_end, l2_start))
        .unwrap();
    assert_eq!(s.connected_chain(l1), sorted(vec![l1, l2]));
    assert_eq!(s.connected_chain(l2), sorted(vec![l1, l2]));
    // Weiter über einen Punkt, der zu keiner Kurve gehört.
    let l3 = free_line(&mut s, (2.0, 0.0), (3.0, 0.0));
    let (l3_start, _) = line_ends(&s, l3);
    let loose = s.add_point(2.0, 0.0).unwrap();
    s.add_constraint(ConstraintKind::Coincident(l2_end, loose))
        .unwrap();
    s.add_constraint(ConstraintKind::Coincident(loose, l3_start))
        .unwrap();
    assert_eq!(s.connected_chain(l1), sorted(vec![l1, l2, l3]));
}

#[test]
fn a_switched_off_coincident_does_not_connect() {
    let mut s = sketch();
    let l1 = free_line(&mut s, (0.0, 0.0), (1.0, 0.0));
    let l2 = free_line(&mut s, (1.0, 0.0), (2.0, 0.0));
    let (_, l1_end) = line_ends(&s, l1);
    let (l2_start, _) = line_ends(&s, l2);
    let k = s
        .add_constraint(ConstraintKind::Coincident(l1_end, l2_start))
        .unwrap();
    s.set_enabled(k, false).unwrap();
    assert_eq!(s.connected_chain(l1), vec![l1]);
}

#[test]
fn arc_ends_connect_but_circles_and_centres_do_not() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let start = s.add_point(2.0, 0.0).unwrap();
    let end = s.add_point(0.0, 2.0).unwrap();
    let arc = s.add_arc(m, start, end).unwrap();
    let far = s.add_point(5.0, 0.0).unwrap();
    let on_end = s.add_line(start, far).unwrap();
    // Mitte des Bogens und des Kreises verbinden nicht, auch nicht über denselben Punkt.
    let far2 = s.add_point(-5.0, 0.0).unwrap();
    let at_centre = s.add_line(m, far2).unwrap();
    let circle = s.add_circle(m, 1.0).unwrap();
    assert_eq!(s.connected_chain(arc), sorted(vec![arc, on_end]));
    assert_eq!(s.connected_chain(on_end), sorted(vec![arc, on_end]));
    assert_eq!(s.connected_chain(at_centre), vec![at_centre]);
    assert_eq!(s.connected_chain(circle), vec![circle]);
}

#[test]
fn branches_run_along_but_point_on_curve_does_not() {
    let mut s = sketch();
    // Drei Linien an einem Punkt: Verzweigungen gehören dazu.
    let hub = s.add_point(0.0, 0.0).unwrap();
    let [p, q, r] = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)].map(|(x, y)| s.add_point(x, y).unwrap());
    let arms = [
        s.add_line(hub, p).unwrap(),
        s.add_line(hub, q).unwrap(),
        s.add_line(hub, r).unwrap(),
    ];
    assert_eq!(s.connected_chain(arms[1]), sorted(arms.to_vec()));
    // T-Stoß: Der Punkt liegt nur auf der Kurve.
    let bar = free_line(&mut s, (0.0, 5.0), (4.0, 5.0));
    let stem = free_line(&mut s, (2.0, 5.0), (2.0, 8.0));
    let (foot, _) = line_ends(&s, stem);
    s.add_constraint(ConstraintKind::PointOnCurve(foot, bar))
        .unwrap();
    assert_eq!(s.connected_chain(bar), vec![bar]);
    assert_eq!(s.connected_chain(stem), vec![stem]);
}

#[test]
fn spline_ends_connect_its_inner_points_do_not() {
    let mut s = sketch();
    let pts = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)].map(|(x, y)| s.add_point(x, y).unwrap());
    let spline = s.add_spline(&pts).unwrap();
    let out = s.add_point(9.0, 9.0).unwrap();
    let at_end = s.add_line(pts[2], out).unwrap();
    let out2 = s.add_point(1.0, 9.0).unwrap();
    let at_inner = s.add_line(pts[1], out2).unwrap();
    assert_eq!(s.connected_chain(spline), sorted(vec![spline, at_end]));
    assert_eq!(s.connected_chain(at_inner), vec![at_inner]);
}

#[test]
fn a_removed_curve_has_no_chain() {
    let mut s = sketch();
    let l = free_line(&mut s, (0.0, 0.0), (1.0, 0.0));
    s.remove_curve(l).unwrap();
    assert!(s.connected_chain(l).is_empty());
}
