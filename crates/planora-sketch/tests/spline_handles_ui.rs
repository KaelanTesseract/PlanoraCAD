//! Was die Oberfläche der Griffe vom Modell braucht (Skizzen 7, Task 13): Lage der gedämpften Griffe,
//! Kette ab einem Griff, Lage des eingefügten Kontrollpunkts, Vergleich zweier Löser-Berichte und die
//! Ablehnung des Krümmungsgriffs neben einem fremden Widerspruch.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use planora_sketch::{HandleCurvature, conic_rho_at, conic_shoulder};

const WAVE: [(f64, f64); 5] = [
    (0.0, 0.0),
    (10.0, 6.0),
    (21.0, -3.0),
    (30.0, 8.0),
    (42.0, 5.0),
];

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn points(s: &mut Sketch, xy: &[(f64, f64)]) -> Vec<PointId> {
    xy.iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect()
}

fn spline(s: &mut Sketch, xy: &[(f64, f64)]) -> (CurveId, Vec<PointId>) {
    let pts = points(s, xy);
    (s.add_spline(&pts).unwrap(), pts)
}

fn pos(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn tip(s: &Sketch, c: CurveId, at: PointId) -> PointId {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == at).unwrap().tip
}

/// Die gedämpfte Spitze liegt genau dort, wo das Einschalten die Spitze anlegt.
#[test]
fn ghost_tips_are_where_activation_puts_the_tip() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let ghosts = s.handle_ghost_tips(c);
    assert_eq!(ghosts.len(), pts.len());
    let (at, ghost) = ghosts[2];
    assert_eq!(at, pts[2]);
    s.activate_tangent_handle(c, at).unwrap();
    assert!(pos(&s, tip(&s, c, at)).distance(ghost) < 1e-12);
    // Ein Griff mehr: Sein Stützpunkt zeigt keinen gedämpften mehr, die anderen bleiben.
    let rest = s.handle_ghost_tips(c);
    assert_eq!(rest.len(), pts.len() - 1);
    assert!(rest.iter().all(|&(p, _)| p != at));
}

/// Die Marke steht auf der Normalen im Abstand der Grifflänge, auf der Seite der Krümmung.
#[test]
fn the_curvature_mark_lies_on_the_normal_on_the_side_of_the_arc() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[2]).unwrap();
    let marks = s.handle_curvature_marks(c);
    assert_eq!(marks.len(), 1);
    let (at, mark) = marks[0];
    assert_eq!(at, pts[2]);
    let (from, to) = (pos(&s, at), pos(&s, tip(&s, c, at)));
    let (forward, side) = (to - from, mark - from);
    assert!(forward.dot(side).abs() < 1e-9 * forward.length_squared());
    assert!((side.length() - forward.length()).abs() < 1e-9);
    s.activate_curvature_handle(c, at).unwrap();
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    let HandleCurvature::Arc { center, .. } = handles[0].curvature else {
        panic!("kein Bogen");
    };
    assert!(
        side.dot(pos(&s, center) - from) > 0.0,
        "Marke auf der Seite der Mitte"
    );
    assert!(
        s.handle_curvature_marks(c).is_empty(),
        "mit Bogen keine Marke"
    );
}

/// Entscheidung: Griffe gehören zu keiner Kette. Es darf keine Rolle spielen, ob der Stützpunkt
/// zufällig ein Ende des Splines ist; vorher nahm die Kette dann den Spline mit, sonst nicht.
#[test]
fn a_chain_from_a_handle_is_only_the_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let inner = s.activate_tangent_handle(c, pts[2]).unwrap();
    let at_end = s.activate_tangent_handle(c, pts[0]).unwrap();
    let arc = s.activate_curvature_handle(c, pts[4]).unwrap();
    assert_eq!(s.connected_chain(inner), vec![inner]);
    assert_eq!(s.connected_chain(at_end), vec![at_end]);
    assert_eq!(s.connected_chain(arc), vec![arc]);
    // Vom Spline aus bleibt es, wie es war: Griffe kommen nicht mit.
    assert_eq!(s.connected_chain(c), vec![c]);
}

/// Der neue Punkt ist der aus der geänderten Gruppe, der der Klickstelle am nächsten liegt.
#[test]
fn the_inserted_control_point_is_the_nearest_of_the_moved_group() {
    let xy = [
        (0.0, 0.0),
        (10.0, 12.0),
        (20.0, -8.0),
        (30.0, 12.0),
        (40.0, -8.0),
        (50.0, 12.0),
        (60.0, 0.0),
        (70.0, 9.0),
    ];
    let mut s = sketch();
    let pts = points(&mut s, &xy);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let curve = s.nurbs_of(c).unwrap();
    let (lo, hi) = curve.knots().domain();
    for k in 1..12 {
        let mut one = s.clone();
        let u = lo + (hi - lo) * f64::from(k) / 12.0;
        let on = curve.point(u).truncate();
        let (new, _) = one.insert_control_point(c, on).unwrap();
        let CurveKind::ControlSpline { points, .. } = &one.curve(c).unwrap().kind else {
            panic!("kein Kontrollpunkt-Spline");
        };
        let moved: Vec<PointId> = points
            .iter()
            .copied()
            .filter(|&p| {
                s.point_xy(p)
                    .is_ok_and(|old| old.distance(pos(&one, p)) > 1e-9)
            })
            .collect();
        let d = pos(&one, new).distance(on);
        for p in moved {
            assert!(
                d <= pos(&one, p).distance(on) + 1e-9,
                "u = {u}: Der neue Punkt liegt {d} entfernt, ein gewanderter näher"
            );
        }
        assert_eq!(points.first(), Some(&pts[0]));
        assert_eq!(points.last(), Some(&pts[7]));
    }
}

/// Ein Zug mit gehaltenen Punkten hinterlässt dem nächsten Lösen keine Zahlen: Unter dem Halten ist die
/// Gruppe kleiner, und das darf nicht als Ergebnis der ganzen Gruppe gelten.
#[test]
fn a_held_drag_leaves_no_numbers_for_the_next_solve() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let free = |r: &SolveReport| {
        let mut f = r.free_points.clone();
        f.sort();
        f
    };
    let before = s.solve();
    let t = tip(&s, c, pts[2]);
    let to = pos(&s, t) + DVec2::new(1.0, 2.0);
    let held = s
        .drag_holding(
            &[Drag {
                point: t,
                target: to.extend(0.0),
                plane: None,
            }],
            &[pts[2]],
        )
        .unwrap();
    assert!(!held.drag_failed);
    assert_eq!(pos(&s, t), to);
    let after = s.solve();
    assert_eq!(after.dof, before.dof);
    assert_eq!(free(&after), free(&before));
    // Auch ein weiteres Lösen, das den Zwischenspeicher trifft, sagt dasselbe.
    let again = s.clone().solve();
    assert_eq!(again.dof, before.dof);
}

/// Der Schulterpunkt und seine Umkehrung sind dieselbe Definition.
#[test]
fn shoulder_and_rho_at_agree() {
    let (p0, a, p2) = (
        DVec2::new(0.0, 0.0),
        DVec2::new(5.0, 9.0),
        DVec2::new(12.0, 1.0),
    );
    for rho in [0.001, 0.25, 0.5, 0.999] {
        let q = conic_shoulder(p0, a, p2, rho);
        assert!((conic_rho_at(p0, a, p2, q).unwrap() - rho).abs() < 1e-12);
    }
    // Scheitel auf der Mitte der Sehne: keine Richtung.
    let m = (p0 + p2) * 0.5;
    assert_eq!(conic_rho_at(p0, m, p2, a), None);
}

fn report(status: SolveStatus, conflicting: &[usize]) -> SolveReport {
    let mut s = sketch();
    let p = s.add_point(0.0, 0.0).unwrap();
    let ids: Vec<_> = conflicting
        .iter()
        .map(|&k| s.add_constraint(K::Fixed(p, k as f64, 0.0)).unwrap())
        .collect();
    let mut r = sketch().solve();
    r.status = status;
    r.conflicting = ids;
    r
}

#[test]
fn only_what_got_worse_counts() {
    use SolveStatus::*;
    let ok = report(Solved, &[]);
    let one = report(Conflicting, &[1]);
    let two = report(Conflicting, &[1, 2]);
    let unsolved = report(NotConverged, &[1]);
    assert!(!ok.is_worse_than(&ok));
    assert!(!ok.is_worse_than(&one), "gelöst ist nie schlechter");
    assert!(one.is_worse_than(&ok));
    assert!(!one.is_worse_than(&one), "derselbe Widerspruch");
    assert!(two.is_worse_than(&one), "ein neuer Widerspruch");
    assert!(!one.is_worse_than(&two), "einer weniger");
    assert!(
        unsolved.is_worse_than(&one),
        "widersprüchlich zu nicht lösbar"
    );
}

/// Quartbogen um (0,0) mit Radius `r` von (r,0) nach (0,r) und ein Spline, der an seinem Ende beginnt.
fn arc_and_spline(s: &mut Sketch) -> (CurveId, CurveId, PointId) {
    let p = points(s, &[(0.0, 0.0), (5.0, 0.0), (0.0, 5.0)]);
    let arc = s.add_arc(p[0], p[1], p[2]).unwrap();
    let q = points(s, &[(-4.0, 6.5), (-8.0, 5.0), (-12.0, 2.0)]);
    let sp = s.add_spline(&[p[2], q[0], q[1], q[2]]).unwrap();
    s.add_constraint(K::Smooth(arc, p[2], sp, p[2])).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    (arc, sp, p[2])
}

/// Ein Widerspruch in einem fremden Teil der Skizze.
fn unrelated_conflict(s: &mut Sketch) {
    let p = s.add_point(100.0, 100.0).unwrap();
    s.add_constraint(K::Fixed(p, 100.0, 100.0)).unwrap();
    s.add_constraint(K::Fixed(p, 200.0, 200.0)).unwrap();
    assert_eq!(s.clone().solve().status, SolveStatus::Conflicting);
}

/// Der fremde Widerspruch hält den Griff nicht auf, und das Lösen läuft trotzdem: Der Griff folgt dem
/// Partner, die Fuge bleibt.
#[test]
fn an_unrelated_conflict_does_not_refuse_the_curvature_handle() {
    let mut s = sketch();
    let (_, sp, joint) = arc_and_spline(&mut s);
    unrelated_conflict(&mut s);
    let bow = s.activate_curvature_handle(sp, joint).unwrap();
    assert!(s.curve(bow).is_some());
    assert_eq!(s.clone().solve().status, SolveStatus::Conflicting);
}

/// Neben dem fremden Widerspruch wird ein neuer trotzdem erkannt. Vorher schaltete der fremde die
/// Prüfung ganz ab, und der Griff blieb mit einem neuen Widerspruch stehen.
#[test]
fn a_new_conflict_beside_an_old_one_is_still_refused() {
    let mut s = sketch();
    let p = points(&mut s, &[(0.0, 0.0), (5.0, 0.0), (0.0, 5.0)]);
    let first = s.add_arc(p[0], p[1], p[2]).unwrap();
    let c2 = points(&mut s, &[(0.0, -5.0), (10.0, -5.0)]);
    let second = s.add_arc(c2[0], c2[1], p[2]).unwrap();
    for q in [p[0], p[1], p[2], c2[0]] {
        let at = pos(&s, q);
        s.add_constraint(K::Fixed(q, at.x, at.y)).unwrap();
    }
    let q = points(&mut s, &[(-4.0, 6.5), (-8.0, 5.0), (-12.0, 2.0)]);
    let sp = s.add_spline(&[p[2], q[0], q[1], q[2]]).unwrap();
    s.add_constraint(K::Smooth(first, p[2], sp, p[2])).unwrap();
    s.add_constraint(K::Smooth(second, p[2], sp, p[2])).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    unrelated_conflict(&mut s);
    let before = s.to_data();
    let e = s.activate_curvature_handle(sp, p[2]).unwrap_err();
    assert!(
        e.to_string()
            .contains(&planora_i18n::tr!("sketch-err-handle-smooth-unsolved")),
        "{e}"
    );
    assert_eq!(s.to_data(), before, "nichts bleibt");
}
