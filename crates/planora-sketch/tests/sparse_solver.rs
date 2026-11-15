//! Dünner Lösungsweg (Skizzen 11a, Task 2): Derselbe Aufbau wird einmal auf dem dichten und
//! einmal auf dem dünnen Weg gelöst; Status, Freiheitsgrade, überzählige und widersprüchliche
//! Abhängigkeiten stimmen überein, die Punkte bis 1e-9.
//! Die Tests setzen den Weg ausdrücklich (Ruling F12), `PLANORA_SOLVER` spielt keine Rolle.

use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveKind, Sketch};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use planora_sketch::sparse::SolverPath;

mod common;
use common::{chain_2d, chain_3d, insert_arc_chain, rect_with_dims, values_bits};

fn same(a: &SolveReport, b: &SolveReport, sa: &Sketch, sb: &Sketch) {
    same_within(a, b, sa, sb, 1e-9);
}

/// Wie [`same`], die Punkte bis `tol`.
fn same_within(a: &SolveReport, b: &SolveReport, sa: &Sketch, sb: &Sketch, tol: f64) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.dof, b.dof);
    assert_eq!(a.redundant, b.redundant);
    assert_eq!(a.conflicting, b.conflicting);
    for (p, _) in sa.points() {
        let d = (sa.point_xyz(p).unwrap() - sb.point_xyz(p).unwrap())
            .abs()
            .max_element();
        assert!(d <= tol, "Punkt {p:?} weicht um {d:e} ab");
    }
}

/// Derselbe Aufbau zweimal, einmal je Weg.
fn both(build: impl Fn() -> Sketch) -> (Sketch, Sketch) {
    let mut dense = build();
    dense.set_solver_path(SolverPath::Dense);
    let mut sparse = build();
    sparse.set_solver_path(SolverPath::Sparse);
    (dense, sparse)
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn sparse_solves_like_dense_2d_chain() {
    let (mut d, mut s) = both(|| {
        let mut s = sketch();
        chain_2d(&mut s, 40);
        s
    });
    let (rd, rs) = (d.solve(), s.solve());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    same(&rd, &rs, &d, &s);
}

#[test]
fn sparse_solves_like_dense_3d_chain() {
    let (mut d, mut s) = both(|| {
        let mut s = sketch();
        chain_3d(&mut s, 20);
        s
    });
    let (rd, rs) = (d.solve(), s.solve());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    same(&rd, &rs, &d, &s);
}

#[test]
fn sparse_drag_like_dense() {
    // Derselbe Aufbau vergibt dieselben Kennungen: das Ende der Kette aus einem Probelauf.
    let (last, _) = chain_2d(&mut sketch(), 40);
    let (mut d, mut s) = both(|| {
        let mut s = sketch();
        chain_2d(&mut s, 40);
        s
    });
    let (rd, rs) = (d.solve(), s.solve());
    same(&rd, &rs, &d, &s);
    let at = d.point_xyz(last).unwrap();
    let drag = [Drag {
        point: last,
        target: at + DVec3::new(0.3, -0.2, 0.0),
        plane: None,
    }];
    let (rd, rs) = (d.drag(&drag).unwrap(), s.drag(&drag).unwrap());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    assert_eq!(rd.drag_failed, rs.drag_failed);
    // Bedenken (Task-2-Bericht): Der Zuglauf der unterbestimmten Kette endet auf beiden Wegen
    // erst an `MAX_ITERATIONS` (100 Schritte, ohne Stillstand). Die Kette hat freie Richtungen,
    // die nur die Dämpfung `λ · s` (λ bis 1e-12) hält; dort wird Rundung bis etwa 1e12-fach
    // verstärkt, und eine andere Eliminationsfolge (RCM) landet an einer anderen, ebenso gültigen
    // Lage. Gemessen: größte Abweichung 6,2e-9 mm. Darum hier 1e-8 statt 1e-9; alles andere
    // (Status, dof, überzählig, Widerspruch) ist gleich.
    same_within(&rd, &rs, &d, &s, 1e-8);
}

#[test]
fn sparse_redundant_like_dense() {
    let (mut d, mut s) = both(|| {
        let mut s = sketch();
        chain_2d(&mut s, 40);
        let (l, a, b) = s
            .curves()
            .find_map(|(id, c)| match c.kind {
                CurveKind::Line { a, b } => Some((id, a, b)),
                _ => None,
            })
            .unwrap();
        s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
            .unwrap();
        s.add_constraint(K::Horizontal(LineOrPoints::Points(a, b)))
            .unwrap();
        s
    });
    let (rd, rs) = (d.solve(), s.solve());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    assert!(!rd.redundant.is_empty(), "{rd:?}");
    same(&rd, &rs, &d, &s);
}

/// Bogenkette mit `n` Bögen; ihre beiden äußersten Punkte stehen fest, und ein Abstand zwischen
/// ihnen verlangt 5 mm mehr, als die festen Lagen zulassen.
fn conflicting_arc_chain(n: usize) -> Sketch {
    let mut s = sketch();
    insert_arc_chain(&mut s, n);
    let pts: Vec<_> = s.points().map(|(id, _)| id).collect();
    let (first, last) = (pts[0], *pts.last().unwrap());
    let (pa, pb) = (s.point_xy(first).unwrap(), s.point_xy(last).unwrap());
    s.add_constraint(K::Fixed(first, pa.x, pa.y)).unwrap();
    s.add_constraint(K::Fixed(last, pb.x, pb.y)).unwrap();
    s.add_constraint(K::Distance(first, last, pa.distance(pb) + 5.0))
        .unwrap();
    s
}

fn conflict_names_same_constraints(n: usize) {
    let (mut d, mut s) = both(|| conflicting_arc_chain(n));
    let (rd, rs) = (d.solve(), s.solve());
    assert_eq!(rd.status, SolveStatus::Conflicting, "{rd:?}");
    assert!(!rd.conflicting.is_empty(), "{rd:?}");
    same(&rd, &rs, &d, &s);
}

/// Prüf-Schwerpunkt 5 mit kleinerer Kette (Ruling F6: läuft auch im Debug-Build).
#[test]
fn sparse_conflict_names_same_constraints() {
    conflict_names_same_constraints(120);
}

/// Prüf-Schwerpunkt 5 in voller Größe (500 Bögen); im Debug-Build zu langsam, bis Task 3 auch
/// die Rangprüfung dünn rechnet.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn sparse_conflict_names_same_constraints_500() {
    conflict_names_same_constraints(500);
}

#[test]
fn small_group_stays_dense() {
    let mut auto = rect_with_dims();
    auto.set_solver_path(SolverPath::Auto);
    let mut dense = rect_with_dims();
    dense.set_solver_path(SolverPath::Dense);
    let (ra, rd) = (auto.solve(), dense.solve());
    assert_eq!(ra.status, SolveStatus::Solved, "{ra:?}");
    assert_eq!(ra.status, rd.status);
    assert_eq!(ra.dof, rd.dof);
    assert_eq!(ra.redundant, rd.redundant);
    assert_eq!(ra.iterations, rd.iterations);
    assert_eq!(values_bits(&auto), values_bits(&dense));
}
