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
    let start_dof = rs.dof;
    let at = d.point_xyz(last).unwrap();
    let drag = [Drag {
        point: last,
        target: at + DVec3::new(0.3, -0.2, 0.0),
        plane: None,
    }];
    let (rd, rs) = (d.drag(&drag).unwrap(), s.drag(&drag).unwrap());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    assert_eq!(rd.drag_failed, rs.drag_failed);
    // Voraussetzung von Ruling T2-1: Beide Wege laufen bis an die Schrittgrenze (100 Schritte),
    // gleich viele Schritte.
    assert_eq!(rd.iterations, rs.iterations, "{rd:?} / {rs:?}");
    assert!(rd.iterations >= 100, "{rd:?}");
    // Bedenken (Task-2-Bericht): Der Zuglauf der unterbestimmten Kette endet auf beiden Wegen
    // erst an `MAX_ITERATIONS` (100 Schritte, ohne Stillstand). Die Kette hat freie Richtungen,
    // die nur die Dämpfung `λ · s` (λ bis 1e-12) hält; dort wird Rundung bis etwa 1e12-fach
    // verstärkt, und eine andere Eliminationsfolge (RCM) landet an einer anderen, ebenso gültigen
    // Lage. Gemessen: größte Abweichung 6,2e-9 mm. Darum hier 1e-8 statt 1e-9; alles andere
    // (Status, dof, überzählig, Widerspruch) ist gleich.
    // A10 (Task 3): Der dünne Weg übernimmt beim Ziehen die Analyse vom Zugbeginn, der dichte
    // rechnet sie an der gezogenen Lage (Ruling F7). Darum hier nur die Lage; die Analyse
    // vergleicht das Lösen beim Loslassen, das auf beiden Wegen neu rechnet.
    assert_eq!(rd.status, rs.status);
    assert_eq!(rd.conflicting, rs.conflicting);
    assert_eq!(rs.dof, start_dof, "{rs:?}");
    for (p, _) in d.points() {
        let e = (d.point_xyz(p).unwrap() - s.point_xyz(p).unwrap())
            .abs()
            .max_element();
        assert!(e <= 1e-8, "Punkt {p:?} weicht um {e:e} ab");
    }
    let (rd, rs) = (d.solve(), s.solve());
    same_within(&rd, &rs, &d, &s, 1e-8);
    assert_eq!(rd.free_points, rs.free_points);
    assert_eq!(rd.free_curves, rs.free_curves);
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

/// Einfacher Zufallsgenerator mit festem Startwert (lineare Kongruenz), Werte in [-1, 1).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }

    fn below(&mut self, n: usize) -> usize {
        (((self.next() + 1.0) * 0.5) * n as f64) as usize % n
    }
}

/// Zufällige Bandmatrix mit gemischt nummerierten Spalten, dazwischen Zeilen, die von früheren
/// abhängen (Summen mit zufälligen Faktoren, eine Kopie, eine Nullzeile). Weniger Zeilen als
/// Spalten, einige Spalten ganz ohne Eintrag: Es gibt einen Nullraum.
fn band_with_dependent_rows(seed: u64) -> (usize, Vec<Vec<(usize, f64)>>) {
    let mut rng = Lcg(seed);
    let n = 30 + rng.below(20);
    let m0 = n - 4 - rng.below(6);
    let mut shuffle: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.below(i + 1);
        shuffle.swap(i, j);
    }
    // Die letzten beiden Spalten (nach dem Mischen) bleiben leer.
    let used = n - 2;
    let mut rows: Vec<Vec<(usize, f64)>> = Vec::new();
    for i in 0..m0 {
        let mid = i * used / m0;
        let row: Vec<(usize, f64)> = (mid.saturating_sub(2)..(mid + 3).min(used))
            .map(|c| (shuffle[c], rng.next()))
            .collect();
        rows.push(row);
        // Ab und zu eine abhängige Zeile aus zwei oder drei der letzten Zeilen.
        if i >= 3 && rng.below(3) == 0 {
            let k = 2 + rng.below(2);
            let mut dep: Vec<(usize, f64)> = Vec::new();
            for t in 0..k {
                let f = rng.next() * 3.0;
                dep.extend(rows[rows.len() - 1 - t].iter().map(|&(c, v)| (c, f * v)));
            }
            rows.push(dep);
        }
    }
    let dup = rows[rng.below(rows.len())].clone();
    rows.push(dup);
    rows.push(Vec::new());
    (n, rows)
}

/// Freie Spalten wie im dichten Löser: ein Nullraumvektor mit einem Anteil über 1e-6.
fn dense_free(space: &planora_sketch::linalg::RowSpace, n: usize) -> Vec<bool> {
    (0..n)
        .map(|c| space.null_space.iter().any(|v| v[c].abs() > 1e-6))
        .collect()
}

#[test]
fn sparse_rank_like_dense() {
    use planora_sketch::linalg::analyze_rows;
    use planora_sketch::sparse::{SparseRows, analyze_rows_sparse, rcm_order};
    for seed in 0..20u64 {
        let (n, rows) = band_with_dependent_rows(0x0a10_0000 + seed);
        let sp = SparseRows::from_rows(n, &rows);
        let perm = rcm_order(&sp);
        let dense = analyze_rows(&sp.to_dense(), 1e-7);
        let sparse = analyze_rows_sparse(&sp, &perm, 1e-7, 1e-6);
        assert_eq!(sparse.independent, dense.independent, "Startwert {seed}");
        assert_eq!(sparse.rank, dense.rank, "Startwert {seed}");
        assert!(
            dense.rank < sp.rows(),
            "Startwert {seed}: keine abhängige Zeile"
        );
        assert_eq!(sparse.free, dense_free(&dense, n), "Startwert {seed}");
        assert!(
            sparse.free.iter().any(|&f| f),
            "Startwert {seed}: nichts frei"
        );
    }
}

#[test]
fn sparse_free_parts_like_dense() {
    let (mut d, mut s) = both(|| {
        let mut s = sketch();
        chain_2d(&mut s, 40);
        let (first, _) = s.points().next().unwrap();
        let at = s.point_xy(first).unwrap();
        s.add_constraint(K::Fixed(first, at.x, at.y)).unwrap();
        s
    });
    let (rd, rs) = (d.solve(), s.solve());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    same(&rd, &rs, &d, &s);
    assert!(!rd.free_points.is_empty(), "{rd:?}");
    assert_eq!(rd.free_points, rs.free_points);
    assert_eq!(rd.free_curves, rs.free_curves);
}

#[test]
fn sparse_parallel_lines_redundant() {
    let (mut d, mut s) = both(|| {
        let mut s = sketch();
        chain_2d(&mut s, 40);
        let lines: Vec<_> = s
            .curves()
            .filter(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
            .map(|(id, _)| id)
            .collect();
        // Zwei gleich gerichtete Linien der Kette (Richtung x, je vier Stücke auseinander).
        let (a, b) = (lines[0], lines[4]);
        s.add_constraint(K::Parallel(a, b)).unwrap();
        s.add_constraint(K::Angle(a, b, 0.0)).unwrap();
        s
    });
    let (rd, rs) = (d.solve(), s.solve());
    assert_eq!(rd.status, SolveStatus::Solved, "{rd:?}");
    assert!(!rd.redundant.is_empty(), "{rd:?}");
    same(&rd, &rs, &d, &s);
    assert_eq!(rd.free_points, rs.free_points);
    assert_eq!(rd.free_curves, rs.free_curves);
}

/// Eine einzige Gruppe mit genau `vars` Variablen: ein Zug aus Punkten mit Abständen, bei
/// ungerader Zahl dazu ein Kreis um den letzten Punkt durch den vorletzten (sein Radius).
fn group_with_vars(vars: usize) -> Sketch {
    let mut s = sketch();
    let k = vars / 2;
    let pts: Vec<_> = (0..k)
        .map(|i| s.add_point(i as f64 * 2.0, (i % 2) as f64 * 0.5).unwrap())
        .collect();
    for w in pts.windows(2) {
        s.add_constraint(K::Distance(w[0], w[1], 2.0)).unwrap();
    }
    if vars % 2 == 1 {
        let c = s.add_circle(pts[k - 1], 2.0).unwrap();
        s.add_constraint(K::PointOnCurve(pts[k - 2], c)).unwrap();
    }
    s
}

#[test]
fn auto_takes_sparse_from_threshold_on() {
    use planora_sketch::sparse::SPARSE_MIN_VARS;
    for (vars, sparse) in [(SPARSE_MIN_VARS - 1, 0), (SPARSE_MIN_VARS, 1)] {
        let mut s = group_with_vars(vars);
        s.set_solver_path(SolverPath::Auto);
        let (r, t) = s.solve_timed();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        assert_eq!(r.group_solves, 1, "{vars} Variablen: eine Gruppe");
        assert_eq!(t.sparse_groups, sparse, "{vars} Variablen");
    }
}
