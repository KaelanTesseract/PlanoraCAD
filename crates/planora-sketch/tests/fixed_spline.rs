//! Fester Spline und projizierte Freiformkanten (Skizzen 7, Task 10; Spec A3, F, Tabelle E, C5).

use planora_i18n::tr;
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::fillet::CurvePick;
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
use planora_sketch::solver::SolveStatus;
use planora_sketch::transform::Similarity;
use planora_sketch::{HandleCurvature, SketchError};
use std::sync::Arc;

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", tol())
}

fn nurbs(degree: usize, knots: &[f64], control: &[(f64, f64, f64)], weights: &[f64]) -> NurbsCurve {
    NurbsCurve::new(
        degree,
        knots.to_vec(),
        control
            .iter()
            .map(|&(x, y, z)| DVec3::new(x, y, z))
            .collect(),
        weights.to_vec(),
    )
    .unwrap()
}

/// Kubisch, offen, nicht rational, mit Höhe.
fn wavy() -> NurbsCurve {
    nurbs(
        3,
        &[0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0],
        &[
            (0.0, 0.0, 1.0),
            (2.0, 5.0, 2.0),
            (5.0, 7.0, -1.0),
            (9.0, 6.0, 3.0),
            (12.0, 3.0, 0.0),
            (14.0, 0.0, 2.0),
        ],
        &[1.0; 6],
    )
}

/// Rational, Grad 2, offen.
fn rational() -> NurbsCurve {
    nurbs(
        2,
        &[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
        &[
            (0.0, 0.0, 0.0),
            (3.0, 6.0, 1.0),
            (7.0, 5.0, 2.0),
            (9.0, 0.0, 1.0),
        ],
        &[1.0, 3.0, 0.4, 1.0],
    )
}

/// Vollkreis mit Radius `r` um den Ursprung, rational, Naht bei `(r, 0)`.
fn circle(r: f64, z: f64) -> NurbsCurve {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    nurbs(
        2,
        &[
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
        &[
            (r, 0.0, z),
            (r, r, z),
            (0.0, r, z),
            (-r, r, z),
            (-r, 0.0, z),
            (-r, -r, z),
            (0.0, -r, z),
            (r, -r, z),
            (r, 0.0, z),
        ],
        &[1.0, w, 1.0, w, 1.0, w, 1.0, w, 1.0],
    )
}

fn free(c: NurbsCurve) -> Curve {
    Curve::Nurbs(Arc::new(c))
}

fn tilted_frame() -> Frame {
    Frame::from_origin_normal(
        DVec3::new(1.0, -2.0, 3.0),
        DVec3::new(1.0, 2.0, 2.0).normalize(),
    )
}

fn fixed_ends(s: &Sketch, c: CurveId) -> (PointId, PointId, Arc<NurbsCurve>) {
    match &s.curve(c).unwrap().kind {
        CurveKind::FixedSpline { start, end, curve } => (*start, *end, curve.clone()),
        k => panic!("kein fester Spline: {k:?}"),
    }
}

fn has_fixed(s: &Sketch, p: PointId) -> bool {
    s.constraints()
        .any(|(_, d)| matches!(d.kind, K::Fixed(q, ..) if q == p))
}

fn solved(s: &mut Sketch, what: &str) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{what}: {r:?}");
}

#[test]
fn projected_nurbs_edge_is_exact() {
    for (curve, what) in [(wavy(), "kubisch"), (rational(), "rational")] {
        for frame in [Frame::WORLD, tilted_frame()] {
            let mut s = Sketch::new(frame, "S", tol());
            let (lo, hi) = curve.domain();
            let edge = free(curve.clone());
            let c = s.add_projected_curve(&edge, lo, hi, LINKED).unwrap();
            let (start, end, stored) = fixed_ends(&s, c);
            assert!(has_fixed(&s, start) && has_fixed(&s, end), "{what}: Fest");
            assert_ne!(start, end);
            assert!(stored.control_points().iter().all(|q| q.z == 0.0), "{what}");
            assert_eq!(stored.weights(), curve.weights(), "{what}: Gewichte");
            assert_eq!(stored.knots().knots(), curve.knots().knots(), "{what}");
            for k in 0..=2000 {
                let t = lo + (hi - lo) * k as f64 / 2000.0;
                let want = frame.to_local(curve.point(t)).truncate();
                let got = s.curve_eval(c, t).unwrap();
                assert!(got.distance(want) < 1e-9, "{what} bei {t}: {got} {want}");
            }
            let want = |t: f64| frame.to_local(curve.point(t)).truncate();
            assert!(s.point_xy(start).unwrap().distance(want(lo)) < 1e-9);
            assert!(s.point_xy(end).unwrap().distance(want(hi)) < 1e-9);
            assert_eq!(s.solve().status, SolveStatus::Solved);
            assert!(
                s.curve_eval(c, lo).unwrap().distance(want(lo)) < 1e-9,
                "Lösen ändert nichts"
            );
        }
    }
}

#[test]
fn sub_range_and_reversed() {
    let curve = wavy();
    let edge = free(curve.clone());
    let project = |t: f64| curve.point(t).truncate();
    let mut s = sketch();
    let up = s.add_projected_curve(&edge, 0.7, 2.2, LINKED).unwrap();
    assert_eq!(s.curve_domain(up).unwrap(), (0.7, 2.2));
    for k in 0..=500 {
        let t = 0.7 + 1.5 * k as f64 / 500.0;
        assert!(s.curve_eval(up, t).unwrap().distance(project(t)) < 1e-9);
    }
    // Absteigend läuft die Kurve vom ersten zum zweiten Parameter.
    let down = s.add_projected_curve(&edge, 2.2, 0.7, LINKED).unwrap();
    let (a, b, _) = fixed_ends(&s, down);
    assert!(s.point_xy(a).unwrap().distance(project(2.2)) < 1e-9);
    assert!(s.point_xy(b).unwrap().distance(project(0.7)) < 1e-9);
    let (lo, hi) = s.curve_domain(down).unwrap();
    for k in 0..=500 {
        let u = k as f64 / 500.0;
        let got = s.curve_eval(down, lo + (hi - lo) * u).unwrap();
        assert!(got.distance(project(2.2 - 1.5 * u)) < 1e-9);
    }
}

#[test]
fn collinear_projection_becomes_a_line() {
    // Liegt in der xz-Ebene und überschießt beide Enden.
    let curve = nurbs(
        3,
        &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        &[
            (0.0, 2.0, 0.0),
            (-3.0, 2.0, 1.0),
            (7.0, 2.0, -1.0),
            (4.0, 2.0, 0.0),
        ],
        &[1.0; 4],
    );
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for k in 0..=200_000 {
        let x = curve.point(k as f64 / 200_000.0).x;
        lo = lo.min(x);
        hi = hi.max(x);
    }
    assert!(lo < -0.3 && hi > 4.3, "die Kurve überschießt: {lo} {hi}");
    let mut s = sketch();
    let edge = free(curve);
    assert!(!s.projects_to_point(&edge, 0.0, 1.0));
    let c = s.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    let CurveKind::Line { a, b } = s.curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    let (pa, pb) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
    assert!((pa.x.min(pb.x) - lo).abs() < 1e-6, "{pa} {pb} {lo}");
    assert!((pa.x.max(pb.x) - hi).abs() < 1e-6, "{pa} {pb} {hi}");
    assert!((pa.y - 2.0).abs() < 1e-6 && (pb.y - 2.0).abs() < 1e-6);
    assert!(has_fixed(&s, a) && has_fixed(&s, b));
}

#[test]
fn point_projection_is_skipped() {
    let up = free(nurbs(
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[(1.0, 1.0, 0.0), (1.0, 1.0, 4.0), (1.0, 1.0, 9.0)],
        &[1.0, 2.0, 1.0],
    ));
    let mut s = sketch();
    assert!(s.projects_to_point(&up, 0.0, 1.0));
    assert!(matches!(
        s.add_projected_curve(&up, 0.0, 1.0, LINKED),
        Err(SketchError::UnsupportedProjection(_))
    ));
    assert_eq!(s.points().count(), 0);
    assert_eq!(s.curves().count(), 0);
    // Eine andere Lage derselben Kurve ist kein Punkt.
    let flat = free(wavy());
    assert!(!s.projects_to_point(&flat, 0.0, 3.0));
}

#[test]
fn closed_edge_gives_one_curve_with_one_point() {
    let mut s = sketch();
    let edge = free(circle(5.0, 2.0));
    let c = s.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    let (start, end, _) = fixed_ends(&s, c);
    assert_eq!(start, end, "ein fester Punkt");
    assert_eq!(s.points().count(), 1);
    assert_eq!(s.curve(c).unwrap().kind.points(), vec![start]);
    assert!(has_fixed(&s, start));
    assert_eq!(
        s.constraints()
            .filter(|(_, d)| matches!(d.kind, K::Fixed(..)))
            .count(),
        1
    );
    assert!(s.point_xy(start).unwrap().distance(DVec2::new(5.0, 0.0)) < 1e-9);
    solved(&mut s, "geschlossen");
    // Der Linienzug schließt, der Abstand stimmt.
    let pl = s.curve_polyline(c, 96).unwrap();
    assert!(pl[0].distance(pl[pl.len() - 1]) < 1e-12);
    for p in &pl {
        assert!((p.length() - 5.0).abs() < 1e-9, "{p}");
    }
}

#[test]
fn closed_edge_over_the_seam_is_exact() {
    // Bereich über die Naht hinaus: Faltung wie im Kernel.
    let curve = circle(5.0, 0.0);
    let edge = free(curve.clone());
    let mut s = sketch();
    let c = s.add_projected_curve(&edge, 0.8, 1.3, LINKED).unwrap();
    let (a, b, stored) = fixed_ends(&s, c);
    assert_ne!(a, b);
    let want = |t: f64| edge.point(t).truncate();
    assert!(s.point_xy(a).unwrap().distance(want(0.8)) < 1e-9);
    assert!(s.point_xy(b).unwrap().distance(want(1.3)) < 1e-9);
    assert_eq!(s.curve_domain(c).unwrap(), (0.8, 1.3));
    assert!(stored.weights().iter().all(|w| *w > 0.0));
    for k in 0..=800 {
        let t = 0.8 + 0.5 * k as f64 / 800.0;
        assert!(s.curve_eval(c, t).unwrap().distance(want(t)) < 1e-9, "{t}");
    }
    // Ganz herum, Anfang nicht an der Naht: ein Punkt.
    let round = s.add_projected_curve(&edge, 0.3, 1.3, LINKED).unwrap();
    let (a, b, _) = fixed_ends(&s, round);
    assert_eq!(a, b);
    assert!(s.point_xy(a).unwrap().distance(want(0.3)) < 1e-9);
    // Eine offene Kurve hat keine Naht.
    let open = free(wavy());
    assert!(s.add_projected_curve(&open, 2.0, 4.0, LINKED).is_err());
}

#[test]
fn closed_fixed_spline_is_a_profile_alone_and_in_the_old_search() {
    let mut s = sketch();
    let edge = free(circle(5.0, 0.0));
    let c = s.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    let new = s.profiles();
    assert_eq!(new.profiles.len(), 1, "{:?}", new.rejected);
    assert_eq!(new.curve_loops[0], vec![c]);
    let old = s.legacy_profiles();
    assert_eq!(old.profiles.len(), 1, "{:?}", old.rejected);
    // Eine Linie durch den Kreis teilt ihn; die Punkte bleiben, wie sie sind.
    let a = s.add_point(-8.0, 1.0).unwrap();
    let b = s.add_point(8.0, 1.0).unwrap();
    s.add_line(a, b).unwrap();
    let split = s.profiles();
    assert_eq!(split.profiles.len(), 2, "{:?}", split.rejected);
}

#[test]
fn reproject_changes_curve_and_values_but_keeps_id() {
    let mut s = sketch();
    let edge = free(wavy());
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    let (a, b, _) = fixed_ends(&s, c);
    let p = s.add_point(7.0, 7.0).unwrap();
    let on = s.add_constraint(K::PointOnCurve(p, c)).unwrap();
    solved(&mut s, "vorher");
    // Andere Form, anderer Grad, andere Knotenzahl, anderer Bereich.
    let other = free(nurbs(
        2,
        &[5.0, 5.0, 5.0, 6.0, 7.0, 7.0, 7.0],
        &[
            (1.0, 1.0, 0.0),
            (3.0, 9.0, 0.0),
            (8.0, 9.0, 0.0),
            (11.0, 2.0, 0.0),
        ],
        &[1.0; 4],
    ));
    let curves = s.curves().count();
    s.reproject_curve(c, &other, 5.0, 7.0).unwrap();
    assert_eq!(s.curves().count(), curves);
    let (a2, b2, stored) = fixed_ends(&s, c);
    assert_eq!((a, b), (a2, b2), "Punkte behalten ihre Id");
    assert_eq!(stored.degree(), 2);
    assert!(s.point_xy(a).unwrap().distance(DVec2::new(1.0, 1.0)) < 1e-9);
    assert!(s.point_xy(b).unwrap().distance(DVec2::new(11.0, 2.0)) < 1e-9);
    let mut fixed = s
        .constraints()
        .filter_map(|(_, d)| match d.kind {
            K::Fixed(q, x, y) => Some((q, x, y)),
            _ => None,
        })
        .collect::<Vec<_>>();
    fixed.sort_by_key(|f| f.0);
    assert_eq!(
        fixed,
        vec![(a, 1.0, 1.0), (b, 11.0, 2.0)],
        "die Fest-Werte folgen"
    );
    assert!(s.constraint(on).is_some(), "die Abhängigkeit bleibt");
    solved(&mut s, "nachher");
    let xy = s.point_xy(p).unwrap();
    assert!(s.nearest_on_curve(c, xy).unwrap().distance(xy) < 1e-6);
    // Art geändert: aus der Freiformkante wird eine Linie.
    let before = s.clone();
    let line = Curve::Line {
        origin: DVec3::ZERO,
        dir: DVec3::X,
    };
    assert!(s.reproject_curve(c, &line, 0.0, 4.0).is_err());
    assert_eq!(fixed_ends(&s, c).2, fixed_ends(&before, c).2, "unverändert");
    // Offen und geschlossen sind verschiedene Lagen der Punkte.
    let round = free(circle(4.0, 0.0));
    assert!(s.reproject_curve(c, &round, 0.0, 1.0).is_err());
    assert_eq!(fixed_ends(&s, c).2, fixed_ends(&before, c).2, "unverändert");
}

#[test]
fn restore_projected_fixed_spline() {
    let mut last = sketch();
    let edge = free(rational());
    let c = last.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    let mut now = last.clone();
    now.reproject_curve(c, &free(wavy()), 0.0, 3.0).unwrap();
    assert_ne!(fixed_ends(&now, c).2, fixed_ends(&last, c).2);
    now.restore_projected_curve(c, &last).unwrap();
    assert_eq!(fixed_ends(&now, c).2, fixed_ends(&last, c).2);
    let (a, b, _) = fixed_ends(&now, c);
    assert_eq!(now.point_xy(a).unwrap(), last.point_xy(a).unwrap());
    assert_eq!(now.point_xy(b).unwrap(), last.point_xy(b).unwrap());
    assert!(
        now.constraints()
            .all(|(id, d)| { last.constraint(id).is_some_and(|o| o.kind == d.kind) })
    );
}

fn no_copy() -> String {
    tr!("sketch-fixed-spline-no-copy")
}

fn says_no_copy<T: std::fmt::Debug>(r: Result<T, SketchError>, what: &str) {
    let e = r.expect_err(what).to_string();
    assert!(e.contains(&no_copy()), "{what}: {e}");
}

#[test]
fn fixed_spline_refuses_trim_copy_mirror_pattern() {
    let mut s = sketch();
    let edge = free(wavy());
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    // Eine Linie quer durch: Stutzen hätte sonst Schnitte.
    let a = s.add_point(6.0, -2.0).unwrap();
    let b = s.add_point(6.0, 12.0).unwrap();
    let cross = s.add_line(a, b).unwrap();
    let at = s.curve_eval(c, 0.6).unwrap();
    let before = format!("{:?}", s.to_data());
    says_no_copy(s.trim(c, at), "Stutzen");
    says_no_copy(s.break_curve(c, at), "Brechen");
    says_no_copy(s.extend(c, s.curve_eval(c, 3.0).unwrap()), "Dehnen");
    assert!(s.trim_piece(c, at).is_none(), "keine Vorschau");
    assert!(
        s.extend_preview(c, s.curve_eval(c, 3.0).unwrap()).is_none(),
        "keine Vorschau"
    );
    says_no_copy(s.add_mirrored(&[c], cross), "Spiegeln");
    says_no_copy(
        s.add_linear_pattern(&[c], DVec2::new(5.0, 0.0), 2),
        "lineares Muster",
    );
    let plan = PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: vec![c],
        counts: [2, 1],
        values: [5.0, 0.0],
        suppressed: Vec::new(),
    };
    says_no_copy(s.add_pattern(&plan), "Muster");
    let set = s.transform_set(&[c], &[]);
    let shift = Similarity {
        offset: DVec2::new(1.0, 0.0),
        ..Similarity::identity(DVec2::ZERO)
    };
    says_no_copy(s.copy_transformed(&set, &shift), "Kopieren");
    assert_eq!(
        before,
        format!("{:?}", s.to_data()),
        "abgelehnt: unverändert"
    );
}

#[test]
fn fixed_spline_cannot_be_moved_or_scaled() {
    for closed in [false, true] {
        let mut s = sketch();
        let edge = if closed {
            free(circle(5.0, 0.0))
        } else {
            free(wavy())
        };
        let (lo, hi) = edge_range(&edge);
        let c = s.add_projected_curve(&edge, lo, hi, LINKED).unwrap();
        let set = s.transform_set(&[c], &[]);
        let (start, _, _) = fixed_ends(&s, c);
        let at = s.point_xy(start).unwrap();
        // Skalieren um den eigenen festen Punkt ließe bei einer geschlossenen Kurve alle Punkte stehen.
        for t in [
            Similarity {
                offset: DVec2::new(1.0, 0.0),
                ..Similarity::identity(at)
            },
            Similarity {
                factor: 2.0,
                ..Similarity::identity(at)
            },
        ] {
            assert!(s.check_transform(&set, &t).is_err(), "{t:?}");
        }
        assert!(
            !s.whole_sketch_set().curves.contains(&c),
            "die ganze Skizze lässt ihn stehen"
        );
    }
}

fn edge_range(edge: &Curve) -> (f64, f64) {
    let Curve::Nurbs(n) = edge else {
        panic!("keine Freiform");
    };
    n.domain()
}

#[test]
fn fixed_spline_cuts_others_and_can_be_deleted() {
    let mut s = sketch();
    let edge = free(wavy());
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    let a = s.add_point(6.0, -2.0).unwrap();
    let b = s.add_point(6.0, 12.0).unwrap();
    let cross = s.add_line(a, b).unwrap();
    // Die Linie wird am festen Spline gestutzt, der Spline bleibt.
    let before = fixed_ends(&s, c).2;
    let parts = s.trim(cross, DVec2::new(6.0, 11.0)).unwrap();
    assert_eq!(parts, vec![cross]);
    assert_eq!(fixed_ends(&s, c).2, before);
    s.remove_curve(c).unwrap();
    assert!(s.curve(c).is_none());
}

#[test]
fn fixed_spline_toggles_construction() {
    let mut s = sketch();
    let edge = free(circle(5.0, 0.0));
    let c = s.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    assert_eq!(s.profiles().profiles.len(), 1);
    s.set_construction(c, true).unwrap();
    assert!(s.curve(c).unwrap().construction);
    assert_eq!(s.profiles().profiles.len(), 0, "kein Profil mehr");
    s.set_construction(c, false).unwrap();
    assert_eq!(s.profiles().profiles.len(), 1);
    // Als Konstruktion angelegt.
    let mut t = sketch();
    let k = t
        .add_projected_curve(&edge, 0.0, 1.0, ProjectStyle { construction: true })
        .unwrap();
    assert!(t.curve(k).unwrap().construction);
}

#[test]
fn offset_of_fixed_spline_is_normal_geometry() {
    let mut s = sketch();
    let edge = free(rational());
    let c = s.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    let chains = s.ordered_chains(&[c]).unwrap();
    let plan = s.offset_plan(&chains, 0.5).unwrap();
    let ids = s.add_offset(&plan).unwrap();
    assert_eq!(ids.curves.len(), 1);
    let copy = ids.curves[0];
    assert!(
        matches!(s.curve(copy).unwrap().kind, CurveKind::Spline { .. }),
        "normale Geometrie"
    );
    solved(&mut s, "Versatz");
    let pl = s.curve_polyline(copy, 96).unwrap();
    for p in &pl[3..pl.len() - 3] {
        let d = s.distance_to_curve(c, *p).unwrap();
        assert!((d - 0.5).abs() < 0.02, "{p}: {d}");
    }
    // Die Kopie lässt sich ändern, der feste Spline nicht.
    let CurveKind::Spline { points, .. } = s.curve(copy).unwrap().kind.clone() else {
        unreachable!();
    };
    assert!(!has_fixed(&s, points[0]));
}

#[test]
fn offset_of_closed_fixed_spline_is_closed() {
    let mut s = sketch();
    let edge = free(circle(5.0, 0.0));
    let c = s.add_projected_curve(&edge, 0.0, 1.0, LINKED).unwrap();
    let chains = s.ordered_chains(&[c]).unwrap();
    let plan = s.offset_plan(&chains, 1.0).unwrap();
    let outlines = plan.outline(96);
    assert_eq!(outlines.len(), 1);
    let line = &outlines[0];
    let radius = line[line.len() / 2].length();
    assert!(
        (radius - 4.0).abs() < 0.05 || (radius - 6.0).abs() < 0.05,
        "{radius}"
    );
    assert!(
        line[0].distance(line[line.len() - 1]) < 1e-6,
        "geschlossen: {} {}",
        line[0],
        line[line.len() - 1]
    );
    let ids = s.add_offset(&plan).unwrap();
    assert_eq!(ids.curves.len(), 1);
    solved(&mut s, "geschlossener Versatz");
    let found = s.profiles();
    assert_eq!(found.profiles.len(), 2, "{:?}", found.rejected);
}

#[test]
fn point_on_fixed_spline_solves() {
    for (curve, seed) in [
        (wavy(), DVec2::new(8.0, 8.0)),
        (rational(), DVec2::new(3.0, 6.0)),
        (circle(5.0, 0.0), DVec2::new(1.0, 6.0)),
    ] {
        let mut s = sketch();
        let (lo, hi) = curve.domain();
        let edge = free(curve);
        let c = s.add_projected_curve(&edge, lo, hi, LINKED).unwrap();
        let before = fixed_ends(&s, c).2;
        let p = s.add_point(seed.x, seed.y).unwrap();
        s.add_constraint(K::PointOnCurve(p, c)).unwrap();
        solved(&mut s, "Punkt auf Kurve");
        let xy = s.point_xy(p).unwrap();
        let d = s.nearest_on_curve(c, xy).unwrap().distance(xy);
        assert!(d < 1e-6, "{d}");
        assert_eq!(fixed_ends(&s, c).2, before);
    }
}

#[test]
fn line_tangent_to_fixed_spline_moves_the_line() {
    let mut s = sketch();
    let edge = free(wavy());
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    let (a, b) = (
        s.add_point(0.0, 9.0).unwrap(),
        s.add_point(14.0, 9.0).unwrap(),
    );
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(K::Tangent(line, c)).unwrap();
    solved(&mut s, "Tangente");
    let (pa, pb) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
    let d = (pb - pa).normalize();
    // Der nächste Punkt auf der Kurve berührt die Linie.
    let lowest = (0..=3000)
        .map(|k| {
            let q = s.curve_eval(c, 3.0 * k as f64 / 3000.0).unwrap();
            (q - pa).perp_dot(d).abs()
        })
        .fold(f64::INFINITY, f64::min);
    assert!(lowest < 1e-4, "{lowest}");
    assert!(
        s.point_xy(fixed_ends(&s, c).0)
            .unwrap()
            .distance(DVec2::ZERO)
            < 1e-9
    );
}

#[test]
fn fillet_keeps_the_fixed_spline_whole() {
    let mut s = sketch();
    let edge = free(wavy());
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    let a = s.add_point(6.0, -2.0).unwrap();
    let b = s.add_point(6.0, 12.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    let pick = |curve: CurveId, at: DVec2| CurvePick { curve, at };
    let at = s.curve_eval(c, 0.6).unwrap();
    let before = fixed_ends(&s, c).2;
    let plan = s
        .fillet_plan(pick(line, DVec2::new(6.0, -1.0)), pick(c, at), 0.5)
        .expect("Rundung");
    s.add_fillet(&plan, None).unwrap();
    assert_eq!(fixed_ends(&s, c).2, before, "Kurve bleibt, wie sie ist");
}

// ---- Glatt (C3, C5) --------------------------------------------------------------------------

/// Sechs-Punkt-Formeln an einem Kurvenende; `side` = +1 vom Anfang des Bereichs aus, sonst −1.
fn ends_of(s: &Sketch, c: CurveId, side: f64) -> (DVec2, DVec2) {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let h = (hi - lo) * 2e-3;
    let t = if side > 0.0 { lo } else { hi };
    let f: [DVec2; 6] = std::array::from_fn(|k| s.curve_eval(c, t + side * k as f64 * h).unwrap());
    let d = (f[0] * (-137.0 / 60.0) + f[1] * 5.0 - f[2] * 5.0 + f[3] * (10.0 / 3.0) - f[4] * 1.25
        + f[5] * 0.2)
        / h
        * side;
    let a = (f[0] * (15.0 / 4.0) - f[1] * (77.0 / 6.0) + f[2] * (107.0 / 6.0) - f[3] * 13.0
        + f[4] * (61.0 / 12.0)
        - f[5] * (5.0 / 6.0))
        / (h * h);
    (d, a)
}

fn at_start(s: &Sketch, c: CurveId, p: PointId) -> bool {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let q = s.point_xy(p).unwrap();
    s.curve_eval(c, lo).unwrap().distance(q) < s.curve_eval(c, hi).unwrap().distance(q)
}

fn kappa_into(s: &Sketch, c: CurveId, p: PointId) -> f64 {
    let first = at_start(s, c, p);
    let (d, a) = ends_of(s, c, if first { 1.0 } else { -1.0 });
    let k = d.perp_dot(a) / d.length().powi(3);
    if first { k } else { -k }
}

fn dir_into(s: &Sketch, c: CurveId, p: PointId) -> DVec2 {
    let first = at_start(s, c, p);
    let (d, _) = ends_of(s, c, if first { 1.0 } else { -1.0 });
    if first { d.normalize() } else { -d.normalize() }
}

fn assert_g2(s: &Sketch, a: CurveId, pa: PointId, b: CurveId, pb: PointId, what: &str) {
    assert!(
        s.point_xy(pa).unwrap().distance(s.point_xy(pb).unwrap()) < 1e-6,
        "{what}: Lage"
    );
    let turn = dir_into(s, a, pa).dot(dir_into(s, b, pb));
    assert!((turn + 1.0).abs() < 1e-8, "{what}: Richtung {turn}");
    let (ka, kb) = (kappa_into(s, a, pa), kappa_into(s, b, pb));
    assert!((ka + kb).abs() < 1e-5, "{what}: κ {ka} und {kb}");
}

/// Glatt wie der Befehl der App: Deckungsgleich, Vorbereiten, die Abhängigkeit.
fn smooth(s: &mut Sketch, fixed: CurveId, at: PointId, spline: CurveId, free_end: PointId) {
    s.add_constraint(K::Coincident(at, free_end)).unwrap();
    s.prepare_smooth(fixed, at, spline, free_end).unwrap();
    s.add_constraint(K::Smooth(fixed, at, spline, free_end))
        .unwrap();
}

#[test]
fn fixed_spline_partner_is_followed() {
    for (curve, what) in [(wavy(), "kubisch"), (rational(), "rational")] {
        for at_end in [false, true] {
            let what = format!("{what}, Ende {at_end}");
            let mut s = sketch();
            let (lo, hi) = curve.domain();
            let edge = free(curve.clone());
            let c = s.add_projected_curve(&edge, lo, hi, LINKED).unwrap();
            let (start, end, before) = fixed_ends(&s, c);
            let at = if at_end { end } else { start };
            let xy = s.point_xy(at).unwrap();
            let q: Vec<PointId> = [(0.0, 0.0), (-3.0, -3.0), (-7.0, -2.0), (-9.0, -6.0)]
                .iter()
                .map(|&(x, y)| s.add_point(xy.x + x, xy.y + y).unwrap())
                .collect();
            let sp = s.add_spline(&q).unwrap();
            smooth(&mut s, c, at, sp, q[0]);
            solved(&mut s, &what);
            // Nur die freie Seite bewegt sich.
            assert_eq!(fixed_ends(&s, c).2, before, "{what}");
            assert_eq!(s.point_xy(at).unwrap(), xy, "{what}");
            assert_g2(&s, c, at, sp, q[0], &what);
            let CurveKind::Spline { handles, .. } = &s.curve(sp).unwrap().kind else {
                unreachable!();
            };
            assert!(
                matches!(handles[0].curvature, HandleCurvature::Arc { .. }),
                "{what}: Griffe am freien Ende"
            );
        }
    }
}

/// Ein natürlicher Spline, kopiert als Kante, hat an seinen Enden die Krümmung null.
#[test]
fn flat_end_of_a_fixed_spline_straightens_the_free_side() {
    let params = [0.0, 1.0, 2.0, 3.0];
    let flat = NurbsCurve::natural_cubic(
        &[
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(3.0, 3.0, 0.0),
            DVec3::new(7.0, 2.0, 0.0),
            DVec3::new(10.0, 5.0, 0.0),
        ],
        &params,
    )
    .unwrap();
    let mut s = sketch();
    let edge = free(flat);
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    let (start, _, _) = fixed_ends(&s, c);
    let q: Vec<PointId> = [(0.0, 0.0), (-3.0, -3.0), (-7.0, -2.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&q).unwrap();
    smooth(&mut s, c, start, sp, q[0]);
    solved(&mut s, "Krümmung null");
    let CurveKind::Spline { handles, .. } = &s.curve(sp).unwrap().kind else {
        unreachable!();
    };
    assert_eq!(handles[0].curvature, HandleCurvature::Straight);
    assert_g2(&s, c, start, sp, q[0], "Krümmung null");
}

#[test]
fn control_spline_adapts_to_a_fixed_spline() {
    let mut s = sketch();
    let edge = free(wavy());
    let c = s.add_projected_curve(&edge, 0.0, 3.0, LINKED).unwrap();
    let (_, end, before) = fixed_ends(&s, c);
    let xy = s.point_xy(end).unwrap();
    let q: Vec<PointId> = [
        (0.0, 0.0),
        (3.0, -3.0),
        (7.0, -2.0),
        (9.0, -6.0),
        (13.0, -5.0),
    ]
    .iter()
    .map(|&(x, y)| s.add_point(xy.x + x, xy.y + y).unwrap())
    .collect();
    let sp = s.add_control_spline(&q, 3).unwrap();
    smooth(&mut s, c, end, sp, q[0]);
    solved(&mut s, "Kontrollpunkt-Spline");
    assert_eq!(fixed_ends(&s, c).2, before);
    assert_g2(&s, c, end, sp, q[0], "Kontrollpunkt-Spline");
}

#[test]
fn two_fixed_splines_refused() {
    let mut s = sketch();
    let (one, two) = (free(wavy()), free(rational()));
    let a = s.add_projected_curve(&one, 0.0, 3.0, LINKED).unwrap();
    let b = s.add_projected_curve(&two, 0.0, 1.0, LINKED).unwrap();
    assert!(s.smooth_partner(a), "er kann Partner sein");
    assert!(!s.smooth_kinds_fit(a, b), "zwei feste gehen nicht");
    let (pa, pb) = (fixed_ends(&s, a).1, fixed_ends(&s, b).0);
    let before = s.constraints().count();
    assert!(s.prepare_smooth(a, pa, b, pb).is_err());
    assert!(s.add_constraint(K::Smooth(a, pa, b, pb)).is_err());
    assert_eq!(s.constraints().count(), before);
    // Gegen einen freien Spline und einen Kegelschnitt: ja. Skizzen 7: gegen eine Linie nicht mehr,
    // siehe `a_line_on_a_fixed_spline_is_refused_with_its_own_reason`.
    let q: Vec<PointId> = [(0.0, 0.0), (1.0, 3.0), (2.0, 9.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&q[..2]).unwrap();
    let conic = s.add_conic(q[0], q[2], q[1], 0.5).unwrap();
    for other in [sp, conic] {
        assert!(s.smooth_kinds_fit(a, other));
    }
}

/// Die Krümmung des festen Splines steht fest, die der Linie ist 0: Die Krümmungszeile ginge nie auf.
/// Darum gibt es den Befehl für dieses Paar nicht, in beiden Wahlreihenfolgen und mit eigenem Grund.
/// Das gilt nach der Art, also auch für ein festes Ende, das zufällig gerade ausläuft.
#[test]
fn a_line_on_a_fixed_spline_is_refused_with_its_own_reason() {
    let flat = NurbsCurve::natural_cubic(
        &[
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(3.0, 3.0, 0.0),
            DVec3::new(7.0, 2.0, 0.0),
            DVec3::new(10.0, 5.0, 0.0),
        ],
        &[0.0, 1.0, 2.0, 3.0],
    )
    .unwrap();
    for (curve, what) in [(wavy(), "krumm"), (flat, "gerades Ende")] {
        let mut s = sketch();
        let (lo, hi) = curve.domain();
        let edge = free(curve);
        let fixed = s.add_projected_curve(&edge, lo, hi, LINKED).unwrap();
        let (_, end, _) = fixed_ends(&s, fixed);
        let xy = s.point_xy(end).unwrap();
        let a = s.add_point(xy.x, xy.y).unwrap();
        let b = s.add_point(xy.x + 5.0, xy.y + 1.0).unwrap();
        let line = s.add_line(a, b).unwrap();
        s.add_constraint(K::Coincident(end, a)).unwrap();
        s.solve();
        let why = SketchError::WrongType(tr!("sketch-smooth-fixed-line"));
        let before = s.to_data();
        for (x, px, y, py) in [(fixed, end, line, a), (line, a, fixed, end)] {
            assert!(!s.smooth_kinds_fit(x, y), "{what}");
            assert_eq!(s.prepare_smooth(x, px, y, py), Err(why.clone()), "{what}");
            assert_eq!(
                s.add_constraint(K::Smooth(x, px, y, py)),
                Err(why.clone()),
                "{what}"
            );
            assert_eq!(s.to_data(), before, "{what}: nichts geändert");
        }
        // Jede für sich bleibt ein Partner, und die Linie darf weiter an einen freien Spline.
        assert!(s.smooth_partner(fixed) && s.smooth_partner(line));
        let q = s.add_point(xy.x - 5.0, xy.y - 1.0).unwrap();
        let sp = s.add_spline(&[a, q]).unwrap();
        assert!(s.smooth_kinds_fit(line, sp), "{what}");
    }
}

// ---- Korrekturrunde 1: Der feste Spline bleibt fest (A3) -----------------------------------------

fn fixed_rows(s: &Sketch, p: PointId) -> Vec<planora_sketch::ConstraintId> {
    s.constraints()
        .filter(|(_, d)| matches!(d.kind, K::Fixed(q, ..) if q == p))
        .map(|(id, _)| id)
        .collect()
}

#[test]
fn fixed_on_an_end_of_a_fixed_spline_stays() {
    for curve in [wavy(), circle(5.0, 0.0)] {
        let mut s = sketch();
        let (lo, hi) = curve.domain();
        let edge = free(curve);
        let c = s.add_projected_curve(&edge, lo, hi, LINKED).unwrap();
        let (start, end, _) = fixed_ends(&s, c);
        for p in [start, end] {
            let id = fixed_rows(&s, p)[0];
            let e = s.remove_constraint(id).unwrap_err().to_string();
            assert!(e.contains(&tr!("sketch-fixed-spline-keep-fixed")), "{e}");
            assert!(s.set_driven(id, true).is_err(), "nicht nur anzeigend");
            assert!(s.set_enabled(id, false).is_err(), "nicht ausgeschaltet");
            let d = s.constraint(id).unwrap();
            assert!(d.enabled && !d.driven);
        }
        // Ohne die Kurve ist es ein gewöhnlicher Fest-Wert.
        let id = fixed_rows(&s, start)[0];
        s.remove_curve(c).unwrap();
        s.remove_constraint(id).unwrap();
    }
    // Ein anderer fester Punkt ist nicht betroffen.
    let mut s = sketch();
    let p = s.add_point(1.0, 1.0).unwrap();
    let id = s.add_constraint(K::Fixed(p, 1.0, 1.0)).unwrap();
    s.remove_constraint(id).unwrap();
}

#[test]
fn loading_checks_ends_and_their_fixed() {
    let mut s = sketch();
    let c = s
        .add_projected_curve(&free(wavy()), 0.0, 3.0, LINKED)
        .unwrap();
    let (start, end, _) = fixed_ends(&s, c);
    let good = s.to_data();
    assert!(Sketch::from_data(good.clone()).is_ok());
    let reason = tr!("sketch-err-data-fixed-spline");
    let refused = |d: planora_sketch::SketchData, what: &str| {
        let e = Sketch::from_data(d).expect_err(what).to_string();
        assert!(e.contains(&reason), "{what}: {e}");
    };
    for p in [start, end] {
        // Der Punkt steht neben dem Kurvenende.
        let mut d = good.clone();
        let x = d.points[p.index()].x;
        d.vars[x] += 0.01;
        refused(d, "neben dem Ende");
        // Sein `Fixed` fehlt, ist gelöscht, anzeigend oder ausgeschaltet.
        let row = fixed_rows(&s, p)[0].index();
        let mut d = good.clone();
        d.constraints[row].alive = false;
        refused(d, "gelöscht");
        let mut d = good.clone();
        d.constraints[row].driven = true;
        refused(d, "anzeigend");
        let mut d = good.clone();
        d.constraints[row].enabled = false;
        refused(d, "ausgeschaltet");
    }
    // Eine gelöschte Kurve prüft niemand mehr: Sie steht nur noch in der Datei.
    let mut gone = s.clone();
    let id = fixed_rows(&gone, start)[0];
    gone.remove_curve(c).unwrap();
    gone.remove_constraint(id).unwrap();
    assert!(Sketch::from_data(gone.to_data()).is_ok());
}

/// Geschlossen, Grad 3, oben und unten gerundet; `tilt` kippt die letzte Tangente an der Naht bei
/// `(5, 0)` um etwa `tilt` rad.
fn loop_with_kink(tilt: f64) -> NurbsCurve {
    nurbs(
        3,
        &[0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0],
        &[
            (5.0, 0.0, 0.0),
            (5.0, 4.0, 0.0),
            (2.0, 7.0, 0.0),
            (-5.0, 3.0, 0.0),
            (-2.0, -7.0, 0.0),
            (5.0 + 4.0 * tilt, -4.0, 0.0),
            (5.0, 0.0, 0.0),
        ],
        &[1.0; 7],
    )
}

fn offset_of(curve: NurbsCurve, d: f64) -> Result<usize, planora_sketch::offset::OffsetProblem> {
    let mut s = sketch();
    let (lo, hi) = curve.domain();
    let c = s.add_projected_curve(&free(curve), lo, hi, LINKED).unwrap();
    let chains = s.ordered_chains(&[c])?;
    s.offset_plan(&chains, d).map(|p| p.piece_count())
}

#[test]
fn closed_curve_with_a_seam_kink_is_refused_clearly() {
    use planora_sketch::offset::OffsetProblem::ClosedKink;
    let teardrop = nurbs(
        3,
        &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        &[
            (0.0, 0.0, 0.0),
            (30.0, -18.0, 0.0),
            (30.0, 18.0, 0.0),
            (0.0, 0.0, 0.0),
        ],
        &[1.0; 4],
    );
    let diamond = nurbs(
        1,
        &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 4.0],
        &[
            (10.0, 0.0, 0.0),
            (0.0, 10.0, 0.0),
            (-10.0, 0.0, 0.0),
            (0.0, -10.0, 0.0),
            (10.0, 0.0, 0.0),
        ],
        &[1.0; 5],
    );
    for (what, curve) in [("Tropfen", teardrop), ("Raute", diamond)] {
        for d in [1.0, -1.0] {
            assert_eq!(offset_of(curve.clone(), d), Err(ClosedKink), "{what} {d}");
        }
    }
    // Kleine Knicke ab etwa 0,001°, beide Richtungen.
    for tilt in [2e-5, 1e-4, 1e-2] {
        for d in [0.5, -0.5] {
            assert_eq!(
                offset_of(loop_with_kink(tilt), d),
                Err(ClosedKink),
                "{tilt} {d}"
            );
        }
    }
}

#[test]
fn closed_curve_with_a_kink_below_tolerance_is_offset() {
    let tol = Tolerance::default();
    // Unter der Winkeltoleranz und um Größenordnungen darüber, solange der Abstand unter `tol.linear` bleibt.
    for tilt in [0.0, tol.angular * 0.5, tol.linear * 0.1] {
        for d in [0.5, -0.5] {
            assert_eq!(offset_of(loop_with_kink(tilt), d), Ok(1), "{tilt} {d}");
        }
    }
}

/// Skizzen 10, Task 5: Einbezogene Freiformkurve im Raum behält ihre Höhe.
#[test]
fn fixed_spline_3d_keeps_z_control_points() {
    use planora_sketch::helpers::{Included, include_curve};
    for frame in [Frame::WORLD, tilted_frame()] {
        let mut s = Sketch::new(frame, "S", tol());
        let edge = free(wavy());
        let g = include_curve(&frame, &edge, 0.0, 3.0).unwrap();
        let Included::Spline { curve } = &g else {
            panic!("Freiformkurve erwartet: {g:?}")
        };
        let local: Vec<DVec3> = wavy()
            .control_points()
            .iter()
            .map(|&p| frame.to_local(p))
            .collect();
        for (a, b) in curve.control_points().iter().zip(&local) {
            assert!(a.distance(*b) < 1e-12, "{a} ≠ {b}");
        }
        let ids = s.add_included(&g).unwrap();
        assert_eq!(ids.len(), 1);
        let c = ids[0];
        let (a, b, kept) = fixed_ends(&s, c);
        assert_ne!(a, b);
        assert!(kept.control_points().iter().any(|q| q.z.abs() > 0.5));
        assert!(s.is_3d_curve(c));
        assert!(s.is_3d_point(a) && s.is_3d_point(b));
        let start = frame.to_local(wavy().point(0.0));
        let end = frame.to_local(wavy().point(3.0));
        assert!(s.point_xyz(a).unwrap().distance(start) < 1e-9);
        assert!(s.point_xyz(b).unwrap().distance(end) < 1e-9);
        for p in [a, b] {
            let at = s.point_xyz(p).unwrap();
            assert!(
                s.constraints()
                    .any(|(_, d)| d.kind == K::Fixed3(p, at.x, at.y, at.z))
            );
            assert!(!has_fixed(&s, p));
        }
        let mid = s.curve_point3(c, 1.5).unwrap();
        assert!(mid.distance(frame.to_local(wavy().point(1.5))) < 1e-9);
        solved(&mut s, "3D-Freiform");
        // Speichern und Laden behält die Höhe.
        let back = Sketch::from_data(s.to_data()).unwrap();
        assert_eq!(fixed_ends(&back, c).2, kept);
        assert!(back.is_3d_point(a));
    }
    // Geschlossen im Raum: ein Punkt für beide Enden.
    let mut s = sketch();
    let g = include_curve(&Frame::WORLD, &free(circle(4.0, 2.0)), 0.0, 1.0).unwrap();
    let c = s.add_included(&g).unwrap()[0];
    let (a, b, _) = fixed_ends(&s, c);
    assert_eq!(a, b);
    assert!((s.point_xyz(a).unwrap().z - 2.0).abs() < 1e-12);
    solved(&mut s, "geschlossen 3D");
    Sketch::from_data(s.to_data()).unwrap();
}
